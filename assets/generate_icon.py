from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter

ROOT = Path(__file__).resolve().parent
SCALE = 2
SIZE = 1024
W = SIZE * SCALE


def sc(value):
    return int(round(value * SCALE))


def vertical_gradient(width, height, stops):
    image = Image.new("RGBA", (width, height))
    pixels = image.load()
    for y in range(height):
        t = y / max(1, height - 1)
        color = stops[-1][1]
        for index in range(len(stops) - 1):
            p0, c0 = stops[index]
            p1, c1 = stops[index + 1]
            if p0 <= t <= p1:
                local = (t - p0) / max(0.0001, p1 - p0)
                color = tuple(int(c0[i] + (c1[i] - c0[i]) * local) for i in range(4))
                break
        for x in range(width):
            pixels[x, y] = color
    return image


def soft_shape_mask(size, draw_fn, blur=0):
    mask = Image.new("L", size, 0)
    draw_fn(ImageDraw.Draw(mask))
    if blur:
        mask = mask.filter(ImageFilter.GaussianBlur(blur))
    return mask


def composite_gradient(canvas, mask, stops):
    gradient = vertical_gradient(canvas.width, canvas.height, stops)
    empty = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    canvas.alpha_composite(Image.composite(gradient, empty, mask))


def bezier_points(p0, p1, p2, p3, count=140):
    points = []
    for i in range(count):
        t = i / (count - 1)
        u = 1 - t
        x = u**3 * p0[0] + 3 * u*u*t * p1[0] + 3 * u*t*t * p2[0] + t**3 * p3[0]
        y = u**3 * p0[1] + 3 * u*u*t * p1[1] + 3 * u*t*t * p2[1] + t**3 * p3[1]
        points.append((sc(x), sc(y)))
    return points


canvas = Image.new("RGBA", (W, W), (0, 0, 0, 0))

ambient_mask = soft_shape_mask(
    canvas.size,
    lambda d: d.rounded_rectangle((sc(88), sc(108), sc(936), sc(956)), radius=sc(210), fill=120),
    sc(48),
)
ambient = Image.new("RGBA", canvas.size, (25, 106, 196, 0))
ambient.putalpha(ambient_mask)
canvas.alpha_composite(ambient)

tile_mask = soft_shape_mask(
    canvas.size,
    lambda d: d.rounded_rectangle((sc(64), sc(64), sc(960), sc(960)), radius=sc(215), fill=255),
)
composite_gradient(
    canvas,
    tile_mask,
    [
        (0.00, (239, 253, 255, 255)),
        (0.24, (190, 239, 255, 255)),
        (0.58, (107, 204, 255, 255)),
        (0.82, (68, 157, 241, 255)),
        (1.00, (118, 204, 247, 255)),
    ],
)

glow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
gd = ImageDraw.Draw(glow)
gd.ellipse((sc(420), sc(-120), sc(1120), sc(560)), fill=(255, 255, 255, 62))
gd.ellipse((sc(-200), sc(520), sc(620), sc(1180)), fill=(90, 242, 235, 60))
glow = glow.filter(ImageFilter.GaussianBlur(sc(70)))
glow.putalpha(Image.composite(glow.getchannel("A"), Image.new("L", canvas.size, 0), tile_mask))
canvas.alpha_composite(glow)

sheen_mask = Image.new("L", canvas.size, 0)
sd = ImageDraw.Draw(sheen_mask)
sd.polygon([(sc(110), sc(310)), (sc(460), sc(70)), (sc(650), sc(70)), (sc(210), sc(420))], fill=105)
sheen_mask = sheen_mask.filter(ImageFilter.GaussianBlur(sc(28)))
sheen_mask = Image.composite(sheen_mask, Image.new("L", canvas.size, 0), tile_mask)
sheen = Image.new("RGBA", canvas.size, (255, 255, 255, 0))
sheen.putalpha(sheen_mask)
canvas.alpha_composite(sheen)

noise = Image.effect_noise(canvas.size, 18).convert("L").point(lambda v: 10 if v > 145 else 0)
noise_alpha = Image.composite(noise, Image.new("L", canvas.size, 0), tile_mask)
noise_layer = Image.new("RGBA", canvas.size, (255, 255, 255, 0))
noise_layer.putalpha(noise_alpha)
canvas.alpha_composite(noise_layer)

draw = ImageDraw.Draw(canvas)
draw.rounded_rectangle((sc(64), sc(64), sc(960), sc(960)), radius=sc(215), outline=(255, 255, 255, 205), width=sc(7))
draw.rounded_rectangle((sc(82), sc(82), sc(942), sc(942)), radius=sc(198), outline=(255, 255, 255, 52), width=sc(3))
draw.arc((sc(94), sc(86), sc(930), sc(930)), start=194, end=342, fill=(255, 255, 255, 165), width=sc(5))

top = (512, 205)
right = (758, 470)
bottom = (512, 716)
left = (266, 470)
kite_points = [(sc(x), sc(y)) for x, y in (top, right, bottom, left)]
left_half = [(sc(x), sc(y)) for x, y in (top, left, bottom)]
right_half = [(sc(x), sc(y)) for x, y in (top, right, bottom)]

kite_shadow_mask = soft_shape_mask(
    canvas.size,
    lambda d: d.polygon([(x + sc(26), y + sc(34)) for x, y in kite_points], fill=150),
    sc(30),
)
kite_shadow = Image.new("RGBA", canvas.size, (24, 105, 196, 82))
kite_shadow.putalpha(kite_shadow_mask)
canvas.alpha_composite(kite_shadow)

left_mask = Image.new("L", canvas.size, 0)
ImageDraw.Draw(left_mask).polygon(left_half, fill=255)
composite_gradient(
    canvas,
    left_mask,
    [
        (0.12, (255, 255, 255, 248)),
        (0.28, (218, 251, 255, 242)),
        (0.55, (121, 225, 251, 238)),
        (0.78, (72, 190, 244, 232)),
        (0.92, (55, 160, 232, 225)),
    ],
)

right_mask = Image.new("L", canvas.size, 0)
ImageDraw.Draw(right_mask).polygon(right_half, fill=255)
composite_gradient(
    canvas,
    right_mask,
    [
        (0.10, (118, 224, 255, 245)),
        (0.32, (62, 183, 250, 243)),
        (0.62, (39, 129, 237, 240)),
        (0.88, (31, 88, 204, 234)),
    ],
)

kite_mask = Image.new("L", canvas.size, 0)
ImageDraw.Draw(kite_mask).polygon(kite_points, fill=255)

shine = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
sh = ImageDraw.Draw(shine)
sh.polygon(
    [(sc(294), sc(392)), (sc(414), sc(238)), (sc(555), sc(218)), (sc(492), sc(454)), (sc(356), sc(510))],
    fill=(255, 255, 255, 132),
)
sh.ellipse((sc(330), sc(232), sc(640), sc(390)), fill=(255, 255, 255, 95))
shine = shine.filter(ImageFilter.GaussianBlur(sc(22)))
shine.putalpha(Image.composite(shine.getchannel("A"), Image.new("L", canvas.size, 0), kite_mask))
canvas.alpha_composite(shine)

inner_mask = Image.new("L", canvas.size, 0)
idraw = ImageDraw.Draw(inner_mask)
idraw.line([sc(280), sc(474), sc(512), sc(694)], fill=120, width=sc(18))
idraw.line([sc(512), sc(694), sc(735), sc(479)], fill=80, width=sc(12))
inner_mask = inner_mask.filter(ImageFilter.GaussianBlur(sc(14)))
inner_mask = Image.composite(inner_mask, Image.new("L", canvas.size, 0), kite_mask)
inner = Image.new("RGBA", canvas.size, (8, 50, 136, 0))
inner.putalpha(inner_mask)
canvas.alpha_composite(inner)

draw = ImageDraw.Draw(canvas)
draw.line([(sc(512), sc(205)), (sc(266), sc(470)), (sc(512), sc(716))], fill=(255, 255, 255, 118), width=sc(5), joint="curve")
draw.line([(sc(512), sc(205)), (sc(758), sc(470)), (sc(512), sc(716))], fill=(255, 255, 255, 92), width=sc(5), joint="curve")
draw.line([(sc(512), sc(205)), (sc(512), sc(716))], fill=(255, 255, 255, 175), width=sc(6))
draw.line([(sc(512), sc(226)), (sc(318), sc(460))], fill=(255, 255, 255, 225), width=sc(8))
draw.line([(sc(512), sc(226)), (sc(548), sc(296))], fill=(255, 255, 255, 165), width=sc(5))

string_points = bezier_points((512, 716), (626, 788), (724, 858), (626, 936))
string_glow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
sg = ImageDraw.Draw(string_glow)
sg.line(string_points, fill=(201, 250, 255, 130), width=sc(20), joint="curve")
string_glow = string_glow.filter(ImageFilter.GaussianBlur(sc(13)))
canvas.alpha_composite(string_glow)
draw = ImageDraw.Draw(canvas)
draw.line(string_points, fill=(247, 255, 255, 235), width=sc(7), joint="curve")
draw.line(string_points, fill=(91, 196, 244, 180), width=sc(2), joint="curve")

tail_points = bezier_points((626, 936), (596, 964), (552, 950), (548, 982))
tail_glow = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
tg = ImageDraw.Draw(tail_glow)
tg.line(tail_points, fill=(122, 224, 255, 120), width=sc(11), joint="curve")
tail_glow = tail_glow.filter(ImageFilter.GaussianBlur(sc(8)))
canvas.alpha_composite(tail_glow)
draw = ImageDraw.Draw(canvas)
draw.line(tail_points, fill=(236, 254, 255, 215), width=sc(4), joint="curve")


def sparkle(cx, cy, radius, alpha):
    layer = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
    d = ImageDraw.Draw(layer)
    pts = [
        (sc(cx), sc(cy - radius)),
        (sc(cx + radius * 0.28), sc(cy - radius * 0.28)),
        (sc(cx + radius), sc(cy)),
        (sc(cx + radius * 0.28), sc(cy + radius * 0.28)),
        (sc(cx), sc(cy + radius)),
        (sc(cx - radius * 0.28), sc(cy + radius * 0.28)),
        (sc(cx - radius), sc(cy)),
        (sc(cx - radius * 0.28), sc(cy - radius * 0.28)),
    ]
    d.polygon(pts, fill=(255, 255, 255, alpha))
    canvas.alpha_composite(layer.filter(ImageFilter.GaussianBlur(sc(7))))
    canvas.alpha_composite(layer)


sparkle(238, 302, 22, 220)
sparkle(786, 622, 16, 180)
sparkle(674, 282, 11, 160)

final_gloss = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
fg = ImageDraw.Draw(final_gloss)
fg.arc((sc(106), sc(96), sc(918), sc(908)), 188, 335, fill=(255, 255, 255, 150), width=sc(4))
fg.arc((sc(120), sc(118), sc(910), sc(900)), 344, 355, fill=(255, 255, 255, 72), width=sc(3))
final_gloss.putalpha(Image.composite(final_gloss.getchannel("A"), Image.new("L", canvas.size, 0), tile_mask))
canvas.alpha_composite(final_gloss)

master = canvas.resize((SIZE, SIZE), Image.Resampling.LANCZOS)
master.save(ROOT / "kite-icon-master.png", optimize=True)
ico_images = [master.resize((size, size), Image.Resampling.LANCZOS) for size in (16, 24, 32, 48, 64, 128, 256)]
ico_images[-1].save(
    ROOT / "kite.ico",
    format="ICO",
    sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)],
    append_images=ico_images[:-1],
)
print("created", ROOT / "kite-icon-master.png", ROOT / "kite.ico")
