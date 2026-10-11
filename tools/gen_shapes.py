#!/usr/bin/env python3
"""Генерує SVG-форми з бібліотеки форм Material 3 Expressive.

Кожна форма задається радіусом як функцією кута r(θ) у полярних
координатах і апроксимується замкненою ламаною.

Запуск із кореня репозиторію: python3 tools/gen_shapes.py
"""

import math
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "ui" / "shapes"


def shape(name, radius, points=360):
    coords = []
    for i in range(points):
        t = 2 * math.pi * i / points
        r = radius(t)
        x = 50 + 50 * r * math.sin(t)
        y = 50 - 50 * r * math.cos(t)
        coords.append(f"{x:.2f} {y:.2f}")
    path = "M" + " L".join(coords) + "Z"
    svg = (
        '<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" '
        f'viewBox="0 0 100 100"><path d="{path}"/></svg>\n'
    )
    (OUT / f"{name}.svg").write_text(svg)


def soft_power(value, exponent):
    return math.copysign(abs(value) ** exponent, value)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    # Печиво з 9 зубцями.
    shape("cookie-9", lambda t: 0.92 + 0.08 * math.cos(9 * t))
    # М'який вибух з 12 променями.
    shape("soft-burst", lambda t: 0.86 + 0.126 * soft_power(math.cos(12 * t), 0.6), 720)
    # Квітка з 8 пелюстками.
    shape("flower-8", lambda t: 0.84 + 0.16 * abs(math.cos(4 * t)) ** 0.8)
    # Чотирилисник.
    shape("clover-4", lambda t: 0.72 + 0.28 * abs(math.cos(2 * t)) ** 0.9)
    # Коло — для плавного морфу до спокійного стану.
    shape("circle", lambda t: 1.0, 120)


if __name__ == "__main__":
    main()
