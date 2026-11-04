#!/usr/bin/env python3
"""Inspect raw framebuffer RGB coverage; creates no edited images."""
from collections import Counter
import json
from pathlib import Path
from PIL import Image

root = Path(__file__).parent
roi = (540, 450, 665, 590)  # isolated green cube, xyxy, same camera and pixels
before = Image.open(root / 'm8-aa-msaa1.png').convert('RGB')
after = Image.open(root / 'm8-aa-msaa4.png').convert('RGB')
base = set(before.crop(roi).get_flattened_data())
counts = Counter(after.crop(roi).get_flattened_data())
background = before.getpixel((540, 450))
partial = sum(count for color, count in counts.items() if color not in base)
adjacent = sum(
    after.getpixel((x, y)) not in base
    and any(after.getpixel((xx, yy)) == background
            for yy in range(y-1, y+2) for xx in range(x-1, x+2))
    for y in range(roi[1], roi[3]) for x in range(roi[0], roi[2])
)
print(json.dumps(dict(roi_xyxy=roi, roi_pixels=(roi[2]-roi[0])*(roi[3]-roi[1]),
                      msaa1_unique_rgb=len(base), msaa4_unique_rgb=len(counts),
                      msaa4_partial_color_pixels=partial,
                      msaa4_partial_pixels_adjacent_to_background=adjacent,
                      base_rgb=sorted(base)), indent=2))
