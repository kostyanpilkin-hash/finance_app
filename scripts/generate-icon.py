"""Generate the Finance app icon: white wallet on an emerald gradient.

Usage: python scripts/generate-icon.py && npx tauri icon src-tauri/icons/icon.png
"""
from PIL import Image, ImageDraw
import os

SIZE = 1024
OUT = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "icons")


def lerp(a, b, t):
    return tuple(int(a[i] + (b[i] - a[i]) * t) for i in range(3))


def make_icon(size):
    s = size / 1024.0
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))

    # Rounded-square diagonal gradient: emerald-700 -> emerald-400
    grad = Image.new("RGB", (size, size))
    gd = ImageDraw.Draw(grad)
    c1, c2 = (4, 120, 87), (52, 211, 153)
    for y in range(size):
        gd.line([(0, y), (size, y)], fill=lerp(c1, c2, y / size))
    mask = Image.new("L", (size, size), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, size - 1, size - 1], radius=int(220 * s), fill=255)
    img.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(img)
    white = (255, 255, 255, 255)
    soft = (167, 243, 208, 255)
    green = (5, 150, 105, 255)

    # Card peeking out of the wallet
    d.rounded_rectangle([300 * s, 220 * s, 760 * s, 470 * s], radius=int(40 * s), fill=soft)

    # Wallet body
    d.rounded_rectangle([190 * s, 330 * s, 834 * s, 800 * s], radius=int(80 * s), fill=white)

    # Clasp flap on the right side
    d.rounded_rectangle([600 * s, 480 * s, 880 * s, 650 * s], radius=int(60 * s), fill=green)
    r = 34 * s
    cx, cy = 690 * s, 565 * s
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=white)

    return img


def main():
    os.makedirs(OUT, exist_ok=True)
    make_icon(SIZE).save(os.path.join(OUT, "icon.png"))
    print("icon.png written to", os.path.abspath(OUT))


if __name__ == "__main__":
    main()
