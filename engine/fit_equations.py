"""Optional local MLX fitter. Only numeric network coefficients leave this process."""
import argparse
import json
import math
from pathlib import Path
import time

import mlx.core as mx
import mlx.nn as nn
import mlx.optimizers as optim
import numpy as np


class CoordinateField(nn.Module):
    def __init__(self):
        super().__init__()
        self.weights, self.biases = [], []
        sizes = [3, 64, 64, 64, 64, 3]
        for i, (inputs, outputs) in enumerate(zip(sizes, sizes[1:])):
            bound = 1 / inputs if i == 0 else math.sqrt(6 / inputs) / 30
            self.weights.append(mx.random.uniform(-bound, bound, (16, outputs, inputs)))
            self.biases.append(mx.random.uniform(-1 / math.sqrt(inputs),
                                                1 / math.sqrt(inputs), (16, 1, outputs)))

    def __call__(self, coordinates):
        for i, (weights, bias) in enumerate(zip(self.weights, self.biases)):
            coordinates = coordinates @ mx.swapaxes(weights, -1, -2) + bias
            if i + 1 < len(self.weights):
                coordinates = mx.sin(30 * coordinates)
        return coordinates


def fit(args):
    started = time.monotonic()
    if not (16 <= args.width <= 256 and 16 <= args.height <= 512
            and args.width % 4 == args.height % 4 == 0):
        raise ValueError("fitting dimensions must be multiples of four within 256x512")
    if not (1 <= args.frames <= 120 and 1 <= args.steps <= 200000 and 1 <= args.fps <= 120):
        raise ValueError("fitting supports 1..120 frames/FPS and 1..200000 steps")
    size = args.width * args.height * args.frames * 3
    if size > 16 * 1024 * 1024 or args.raw.stat().st_size != size:
        raise ValueError("invalid or oversized RGB training data")
    mx.random.seed(7)
    rgb = np.fromfile(args.raw, dtype=np.uint8).reshape(args.frames, args.height, args.width, 3)
    pw, ph = args.width // 4, args.height // 4
    samples = args.frames * pw * ph
    tiles = rgb.reshape(args.frames, 4, ph, 4, pw, 3).transpose(1, 3, 0, 2, 4, 5).reshape(16, samples, 3)
    targets = mx.array(tiles.astype(np.float32) / 127.5 - 1)
    model = CoordinateField()
    optimizer = optim.Adam(learning_rate=1e-4)
    state = [model.state, optimizer.state, mx.random.state]

    def coordinates(index):
        x = (index % pw).astype(mx.float32)
        y = ((index // pw) % ph).astype(mx.float32)
        t = (index // (pw * ph)).astype(mx.float32)
        return mx.stack([x * 2 / (pw - 1) - 1, y * 2 / (ph - 1) - 1,
                         t * 2 / max(args.frames - 1, 1) - 1], axis=-1)

    def loss(network, x, y):
        return mx.mean(mx.square(network(x) - y))

    gradient = nn.value_and_grad(model, loss)

    def update():
        index = mx.random.randint(0, samples, (16, 512))
        value, grads = gradient(model, coordinates(index), targets[mx.arange(16)[:, None], index])
        optimizer.update(model, grads)
        return value

    step = mx.compile(update, inputs=state, outputs=state)
    mx.eval(model.parameters(), optimizer.state)
    print(f"Fitting coordinate functions on {mx.default_device()}: {args.frames} frames, "
          f"{args.width}x{args.height}, {args.steps} steps", flush=True)
    for i in range(args.steps):
        optimizer.learning_rate = mx.array(1e-4 * (0.05 + 0.95 * 0.5 * (1 + math.cos(math.pi * i / args.steps))))
        value = step()
        mx.eval(value, state)
        if (i + 1) % 1000 == 0 or i + 1 == args.steps:
            psnr = -10 * math.log10(max(float(value.item()) / 4, 1e-12))
            print(f"step {i + 1}: sampled {psnr:.2f} dB, {time.monotonic() - started:.1f}s", flush=True)

    layers = []
    for i in range(len(model.weights)):
        weights, bias = np.asarray(model.weights[i]), np.asarray(model.biases[i])
        scale = max(float(np.abs(weights).max()), float(np.abs(bias).max()), 1e-10) / 2047
        qw = np.rint(weights / scale).astype(np.int16)
        qb = np.rint(bias / scale).astype(np.int16)
        model.weights[i] = mx.array(qw.astype(np.float32) * scale)
        model.biases[i] = mx.array(qb.astype(np.float32) * scale)
        layers.append(dict(inputs=weights.shape[2], outputs=weights.shape[1], scale=scale,
                           weights=qw.reshape(-1).tolist(), bias=qb.reshape(-1).tolist()))

    # Evaluate all frames after quantization; sampled training loss is not validation.
    reconstruction = np.empty((16, samples, 3), np.uint8)
    for first in range(0, samples, 1024):
        last = min(first + 1024, samples)
        index = mx.broadcast_to(mx.arange(first, last)[None, :], (16, last - first))
        prediction = mx.clip((model(coordinates(index)) + 1) * 127.5, 0, 255)
        reconstruction[:, first:last] = np.asarray(mx.round(prediction).astype(mx.uint8))
    reconstruction = reconstruction.reshape(4, 4, args.frames, ph, pw, 3).transpose(2, 0, 3, 1, 4, 5).reshape(rgb.shape)
    error = np.square(rgb.astype(np.float32) - reconstruction.astype(np.float32))
    mse = float(error.mean())
    psnr = 10 * math.log10(255 ** 2 / max(mse, 1e-12))
    minimum = float((10 * np.log10(255 ** 2 / np.maximum(error.mean((1, 2, 3)), 1e-12))).min())
    result = dict(width=args.width, height=args.height, frames=args.frames, fps=args.fps,
                  tiles=4, omega=30.0, layers=layers, psnr=psnr, min_frame_psnr=minimum,
                  parameters=sum(len(l["weights"]) + len(l["bias"]) for l in layers),
                  fit_seconds=time.monotonic() - started)
    args.output.write_text(json.dumps(result, separators=(",", ":")), encoding="utf-8")
    print(f"Quantized model: {result['parameters']} coefficients, full-grid {psnr:.2f} dB, "
          f"worst frame {minimum:.2f} dB", flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("raw", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--width", type=int, required=True)
    parser.add_argument("--height", type=int, required=True)
    parser.add_argument("--frames", type=int, required=True)
    parser.add_argument("--fps", type=int, required=True)
    parser.add_argument("--steps", type=int, default=45000)
    fit(parser.parse_args())
