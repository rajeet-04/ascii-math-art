HAPPY DIWALI
an animated ASCII artwork by rajeet-04, 2026

Fireworks over a dark city and water, drawn only with letters and symbols,
ending in a greeting written by the stars of the last shell.

Files
  diwali.py          the artwork (one file)
  requirements.txt   numpy, pygame-ce, pillow
  HOW_IT_WORKS.txt   the maths and the drawing, step by step
  LICENSE.txt        terms of use
  images/            stills and a preview GIF

Run it
  With uv (recommended), from this folder:

      uv run --with-requirements requirements.txt diwali.py

  Without uv:

      pip install -r requirements.txt
      python diwali.py

Keys
  Space   pause / resume
  R       replay from the start
  S       save the current picture as a PNG next to diwali.py
  Esc     quit

Notes
  One run lasts 26 seconds and then loops. It targets 30 frames per second;
  a frame takes about 20 ms on an Apple-silicon laptop. On smaller screens
  the window scales down to fit. Python 3.10 or newer.

  Free for personal, non-commercial use. See LICENSE.txt.
  github.com/rajeet-04
