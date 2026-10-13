# Original Wii U Connect icon: a phone (controller) with a D-pad and a signal arc. No third-party artwork.
import sys
from PIL import Image, ImageDraw

def draw(size):
    s = size * 4  # supersample
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    r = s * 0.22
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=r, fill=(31, 165, 230, 255))
    # phone body
    pw, ph = s * 0.42, s * 0.70
    px, py = (s - pw) / 2, s * 0.17
    d.rounded_rectangle([px, py, px + pw, py + ph], radius=s * 0.08, fill=(255, 255, 255, 255))
    # D-pad
    cx, cy, a, t = s / 2, py + ph * 0.36, s * 0.13, s * 0.05
    ink = (38, 50, 60, 255)
    d.rectangle([cx - a, cy - t, cx + a, cy + t], fill=ink)
    d.rectangle([cx - t, cy - a, cx + t, cy + a], fill=ink)
    # A button
    br = s * 0.055
    by = py + ph * 0.72
    d.ellipse([cx - br, by - br, cx + br, by + br], fill=(31, 165, 230, 255))
    return img.resize((size, size), Image.LANCZOS)

sizes = [16, 24, 32, 48, 64, 128, 256]
big = draw(256)
big.save(sys.argv[1], format="ICO", sizes=[(n, n) for n in sizes])
draw(512).save(sys.argv[2])
