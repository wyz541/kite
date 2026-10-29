from __future__ import annotations

import io
import math
import os
import platform
import shutil
import struct
import subprocess
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw, ImageFilter, ImageFont

REPO_ROOT = Path(__file__).resolve().parents[2]
ASSETS = REPO_ROOT / "assets"
SVG_SOURCE = Path(__file__).with_name("kite-icon.svg")
MASTER_PATH = ASSETS / "kite-icon-master.png"
ICO_PATH = ASSETS / "kite.ico"
ICNS_PATH = ASSETS / "kite.icns"
PREVIEW_PATH = ASSETS / "kite-icon-preview.png"

CANVAS_SIZE = 1024
ICO_SIZES = (16, 24, 32, 48, 64, 128, 256)
ICNS_TYPES = (
    ("icp4", 16),
    ("icp5", 32),
    ("icp6", 64),
    ("ic07", 128),
    ("ic08", 256),
    ("ic09", 512),
    ("ic10", 1024),
    ("ic11", 32),
    ("ic12", 64),
    ("ic13", 256),
    ("ic14", 512),
)


def browser_candidates():
    configured = os.environ.get("KITE_ICON_BROWSER")
    if configured:
        yield Path(configured)

    system = platform.system()
    if system == "Windows":
        for path in (
            Path(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
            Path(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
            Path(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
            Path(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        ):
            yield path
    elif system == "Darwin":
        for path in (
            Path("/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"),
            Path("/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge"),
            Path("/Applications/Chromium.app/Contents/MacOS/Chromium"),
        ):
            yield path

    for command in (
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
        "chrome",
    ):
        resolved = shutil.which(command)
        if resolved:
            yield Path(resolved)


def find_browser() -> Path:
    for candidate in browser_candidates():
        if candidate.is_file():
            return candidate
    raise RuntimeError(
        "Chrome, Edge, or Chromium is required to render the SVG master. "
        "Set KITE_ICON_BROWSER to the browser executable."
    )


def render_master() -> Image.Image:
    if not SVG_SOURCE.is_file():
        raise RuntimeError(f"missing SVG icon source: {SVG_SOURCE}")

    browser = find_browser()
    with tempfile.TemporaryDirectory(prefix="kite-icon-") as temp_dir:
        temp = Path(temp_dir)
        screenshot = temp / "kite-icon.png"
        profile = temp / "browser-profile"
        command = [
            str(browser),
            "--headless=new",
            "--disable-gpu",
            "--disable-extensions",
            "--disable-background-networking",
            "--hide-scrollbars",
            "--no-first-run",
            "--force-device-scale-factor=1",
            "--default-background-color=00000000",
            f"--window-size={CANVAS_SIZE},{CANVAS_SIZE}",
            f"--user-data-dir={profile}",
            f"--screenshot={screenshot}",
            SVG_SOURCE.as_uri(),
        ]
        result = subprocess.run(command, capture_output=True, text=True, timeout=60)
        if result.returncode != 0:
            message = result.stderr.strip() or result.stdout.strip()
            raise RuntimeError(f"browser failed to render SVG: {message}")
        if not screenshot.is_file():
            raise RuntimeError("browser did not create the icon screenshot")

        image = Image.open(screenshot).convert("RGBA")
        if image.size != (CANVAS_SIZE, CANVAS_SIZE):
            raise RuntimeError(f"unexpected screenshot size: {image.size}")
        if image.getpixel((0, 0))[3] != 0:
            raise RuntimeError("rendered icon is not transparent at the canvas edge")
        return image


def write_icns(master: Image.Image) -> None:
    chunks = []
    for kind, size in ICNS_TYPES:
        payload_buffer = io.BytesIO()
        master.resize((size, size), Image.Resampling.LANCZOS).save(
            payload_buffer,
            format="PNG",
            optimize=True,
        )
        payload = payload_buffer.getvalue()
        chunks.append(kind.encode("ascii") + struct.pack(">I", len(payload) + 8) + payload)
    body = b"".join(chunks)
    ICNS_PATH.write_bytes(b"icns" + struct.pack(">I", len(body) + 8) + body)


def font_candidates(bold: bool):
    if platform.system() == "Windows":
        fonts = Path(r"C:\Windows\Fonts")
        return (
            fonts / ("segoeuib.ttf" if bold else "segoeui.ttf"),
            fonts / ("arialbd.ttf" if bold else "arial.ttf"),
        )
    if platform.system() == "Darwin":
        return (
            Path("/System/Library/Fonts/SFNS.ttf"),
            Path("/System/Library/Fonts/Helvetica.ttc"),
        )
    return (
        Path("/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"),
        Path("/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf" if bold else "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf"),
    )


def load_font(size: int, bold: bool = False) -> ImageFont.FreeTypeFont | ImageFont.ImageFont:
    for candidate in font_candidates(bold):
        if candidate.is_file():
            try:
                return ImageFont.truetype(str(candidate), size=size)
            except OSError:
                pass
    return ImageFont.load_default()


def rounded_panel(draw: ImageDraw.ImageDraw, box, radius: int, fill):
    draw.rounded_rectangle(tuple(int(value) for value in box), radius=radius, fill=fill)


def preview_card(master: Image.Image, board: Image.Image, box, dark: bool, label: str):
    draw = ImageDraw.Draw(board)
    rounded_panel(draw, box, 30, (9, 20, 44, 255) if dark else (250, 247, 240, 255))
    x0, y0, x1, y1 = box
    icon_size = 410
    icon = master.resize((icon_size, icon_size), Image.Resampling.LANCZOS)
    icon_x = x0 + (x1 - x0 - icon_size) // 2
    icon_y = y0 + 28
    board.alpha_composite(icon, (icon_x, icon_y))

    label_font = load_font(17, bold=True)
    note_font = load_font(15)
    text_color = (231, 238, 255) if dark else (81, 74, 67)
    note_color = (150, 169, 205) if dark else (135, 124, 113)
    draw.text((x0 + 34, y1 - 78), label, font=label_font, fill=text_color)
    note = "Full contrast on dark surfaces" if dark else "Full contrast on light surfaces"
    draw.text((x0 + 34, y1 - 49), note, font=note_font, fill=note_color)


def scale_row(master: Image.Image, board: Image.Image, box, dark: bool):
    x0, y0, x1, y1 = box
    draw = ImageDraw.Draw(board)
    rounded_panel(draw, box, 24, (9, 20, 44, 255) if dark else (250, 247, 240, 255))
    label_color = (222, 232, 250) if dark else (86, 77, 69)
    muted_color = (137, 158, 198) if dark else (143, 132, 121)
    label_font = load_font(14)
    draw.text((x0 + 28, y0 + 20), "DARK" if dark else "LIGHT", font=label_font, fill=muted_color)

    sizes = (128, 64, 48, 32, 24, 16)
    positions = (x0 + 178, x0 + 382, x0 + 550, x0 + 696, x0 + 820, x0 + 930)
    for size, x in zip(sizes, positions):
        icon = master.resize((size, size), Image.Resampling.LANCZOS)
        top = y0 + 24 + (128 - size) // 2
        board.alpha_composite(icon, (x - size // 2, top))
        text = f"{size}px"
        text_box = draw.textbbox((0, 0), text, font=label_font)
        draw.text((x - (text_box[2] - text_box[0]) // 2, y1 - 28), text, font=label_font, fill=label_color)

    draw.text((x1 - 150, y0 + 20), "SVG / PNG / ICO / ICNS", font=label_font, fill=muted_color)


def build_preview(master: Image.Image) -> Image.Image:
    board = Image.new("RGBA", (1600, 1320), (240, 236, 229, 255))
    draw = ImageDraw.Draw(board)

    draw.ellipse((-320, -420, 760, 580), fill=(250, 216, 188, 92))
    draw.ellipse((890, -440, 1940, 520), fill=(190, 203, 227, 105))
    draw.ellipse((-220, 790, 780, 1580), fill=(220, 205, 188, 88))

    title_font = load_font(50, bold=True)
    subtitle_font = load_font(23)
    chip_font = load_font(16, bold=True)
    draw.text((64, 48), "Kite / Twin Flow", font=title_font, fill=(16, 33, 62))
    draw.text(
        (65, 114),
        "Orange and navy tempered glass, twin data ribbons, and a machined steel spool.",
        font=subtitle_font,
        fill=(99, 102, 98),
    )
    rounded_panel(draw, (1324, 54, 1536, 98), 22, (255, 252, 246, 235))
    draw.text((1365, 66), "NEW CONCEPT", font=chip_font, fill=(198, 76, 28))

    preview_card(master, board, (64, 182, 784, 772), False, "LIGHT SURFACE")
    preview_card(master, board, (816, 182, 1536, 772), True, "DARK SURFACE")

    section_font = load_font(32, bold=True)
    draw.text((64, 812), "Scale ladder", font=section_font, fill=(16, 33, 62))
    draw.text(
        (64, 860),
        "Tested from 128px down to 16px on both surfaces.",
        font=load_font(17),
        fill=(107, 108, 102),
    )
    scale_row(master, board, (64, 902, 1536, 1087), False)
    scale_row(master, board, (64, 1099, 1536, 1284), True)

    return board.convert("RGB")


def main() -> None:
    master = render_master()
    MASTER_PATH.parent.mkdir(parents=True, exist_ok=True)
    master.save(MASTER_PATH, format="PNG", optimize=True)
    master.save(ICO_PATH, format="ICO", sizes=[(size, size) for size in ICO_SIZES])
    write_icns(master)
    build_preview(master).save(PREVIEW_PATH, format="PNG", optimize=True)

    print(f"source  {SVG_SOURCE}")
    print(f"master  {MASTER_PATH}")
    print(f"windows {ICO_PATH}")
    print(f"macos   {ICNS_PATH}")
    print(f"preview {PREVIEW_PATH}")


if __name__ == "__main__":
    main()
