import gc
import json
import os
import sys

import fontforge
import psMat

try:
    sys.stdout.reconfigure(encoding="utf-8", errors="replace", line_buffering=True)
    sys.stderr.reconfigure(encoding="utf-8", errors="replace", line_buffering=True)
except Exception:
    pass

NORMAL_WEIGHTS = [
    ("Light", "segoeuil.ttf"),
    ("Semilight", "segoeuisl.ttf"),
    ("Regular", "segoeui.ttf"),
    ("Semibold", "seguisb.ttf"),
    ("Bold", "segoeuib.ttf"),
    ("Black", "seguibl.ttf"),
]

ITALIC_WEIGHTS = [
    ("Light Italic", "seguili.ttf"),
    ("Semilight Italic", "seguisli.ttf"),
    ("Italic", "segoeuii.ttf"),
    ("Semibold Italic", "seguisbi.ttf"),
    ("Bold Italic", "segoeuiz.ttf"),
    ("Black Italic", "seguibli.ttf"),
]

# Glyphs that Windows needs in system fonts (essential for Windows functionality)
ESSENTIAL_GLYPHS = {
    *range(0x0020, 0x007F),   # Basic ASCII & Latin
    *range(0x0100, 0x0250),   # Latin Extended-A & B
    *range(0x1E00, 0x1F00),   # Latin Extended Additional
    *range(0x2000, 0x2070),   # General Punctuation
    *range(0x2070, 0x20A0),   # Superscripts & Subscripts
    *range(0x20A0, 0x20D0),   # Currency Symbols
    *range(0x2100, 0x2150),   # Letterlike Symbols
    *range(0x2150, 0x2190),   # Number Forms
    *range(0x2190, 0x2200),   # Arrows
    *range(0x2200, 0x2300),   # Mathematical Operators
    *range(0x2300, 0x2400),   # Miscellaneous Technical
    *range(0x2500, 0x2580),   # Box Drawing
    *range(0x2580, 0x25A0),   # Block Elements
    *range(0x25A0, 0x2600),   # Geometric Shapes
    *range(0x2600, 0x2700),   # Miscellaneous Symbols
    *range(0x2700, 0x27C0),   # Dingbats
    *range(0x0370, 0x0400),   # Greek & Coptic
    *range(0x0400, 0x0500),   # Cyrillic
    *range(0x0600, 0x0700),   # Arabic
    *range(0x1780, 0x1800),   # Khmer
    *range(0x19E0, 0x1A00),   # Khmer Symbols
    *range(0x0750, 0x0780),   # Arabic Supplement
    *range(0x08A0, 0x0900),   # Arabic Extended-A
    *range(0xFB50, 0xFE00),   # Arabic Presentation Forms-A
    *range(0xFE70, 0xFF00),   # Arabic Presentation Forms-B
    *range(0x0590, 0x0600),   # Hebrew
    *range(0x3000, 0x3040),   # CJK Symbols & Punctuation
    *range(0x3040, 0x30A0),   # Hiragana
    *range(0x30A0, 0x3100),   # Katakana
    *range(0x3130, 0x3190),   # Hangul Compatibility Jamo
    *range(0x4E00, 0x4FFF),   # CJK Unified Ideographs (subset)
}


def _load_extra_ranges():
    ranges = []
    try:
        for s, e in json.loads(os.environ.get("SEGOEUI_EXTRA_RANGES", "[]")):
            ranges.append((int(s), int(e)))
    except Exception:
        pass
    return ranges


EXTRA_RANGES = _load_extra_ranges()
for _s, _e in EXTRA_RANGES:
    ESSENTIAL_GLYPHS.update(range(_s, _e + 1))

try:
    EXTRA_FONT_MAP = json.loads(os.environ.get("SEGOEUI_EXTRA_FONT_MAP", "{}"))
except Exception:
    EXTRA_FONT_MAP = {}


def _glyph_bbox_height(font, codepoint):
    try:
        g = font[codepoint]
        if g is None:
            return 0
        bbox = g.boundingBox()
        if not bbox:
            return 0
        return max(0, bbox[3] - bbox[1])
    except Exception:
        return 0


def _extract_segoe_info(segoe_font):
    """Captures metrics and Windows metadata from an already-open Segoe UI font."""
    lat_h = _glyph_bbox_height(segoe_font, 0x48) or segoe_font.ascent * 0.65
    ara_h = _glyph_bbox_height(segoe_font, 0x0627) or segoe_font.ascent * 0.65
    return {
        "em": segoe_font.em,
        "ascent": segoe_font.ascent,
        "descent": segoe_font.descent,
        "hhea_ascent": segoe_font.hhea_ascent,
        "hhea_descent": segoe_font.hhea_descent,
        "hhea_linegap": segoe_font.hhea_linegap,
        "os2_typoascent": segoe_font.os2_typoascent,
        "os2_typodescent": segoe_font.os2_typodescent,
        "os2_winascent": segoe_font.os2_winascent,
        "os2_windescent": segoe_font.os2_windescent,
        "lat_ref_h": lat_h,
        "ara_ref_h": ara_h,
        "fontname": segoe_font.fontname,
        "familyname": segoe_font.familyname,
        "fullname": segoe_font.fullname,
        "sfnt_names": segoe_font.sfnt_names,
        "os2_weight": segoe_font.os2_weight,
        "os2_stylemap": segoe_font.os2_stylemap,
        "macstyle": segoe_font.macstyle,
    }


def _apply_segoe_metadata(final_font, info):
    """Applies Windows system font identity and vertical metrics to final_font."""
    scale = (final_font.em / float(info["em"])) if info.get("em") else 1.0
    final_font.fontname = info["fontname"]
    final_font.familyname = info["familyname"]
    final_font.fullname = info["fullname"]
    final_font.sfnt_names = info["sfnt_names"]
    final_font.os2_weight = info["os2_weight"]
    final_font.os2_stylemap = info["os2_stylemap"]
    final_font.ascent = int(round(info["ascent"] * scale))
    final_font.descent = int(round(info["descent"] * scale))
    final_font.hhea_ascent = int(round(info["hhea_ascent"] * scale))
    final_font.hhea_descent = int(round(info["hhea_descent"] * scale))
    final_font.hhea_linegap = int(round(info["hhea_linegap"] * scale))
    final_font.os2_typoascent = int(round(info["os2_typoascent"] * scale))
    final_font.os2_typodescent = int(round(info["os2_typodescent"] * scale))
    final_font.os2_winascent = int(round(info["os2_winascent"] * scale))
    final_font.os2_windescent = int(round(info["os2_windescent"] * scale))
    final_font.macstyle = info["macstyle"]


def _cleanup_unused_glyphs(font, preserve_arabic_joining=False, remove_kern_lookups=True):
    """Removes problematic variation-selector/zero-width glyphs while keeping combining marks."""
    try:
        if remove_kern_lookups and not preserve_arabic_joining:
            for lookup in list(font.gpos_lookups):
                if "kern" in lookup.lower():
                    try:
                        font.removeLookup(lookup)
                    except Exception:
                        pass
        if not preserve_arabic_joining:
            for lookup in list(font.gsub_lookups):
                try:
                    font.removeLookup(lookup)
                except Exception:
                    pass

        problematic = {
            "uni180B", "uni180C", "uni180D", "uni180E",
            "uni2060", "uni2061", "uni2062", "uni2063", "uni2064",
            "uni2065", "uni2066", "uni2067", "uni2068", "uni2069",
            "uni206A", "uni206B", "uni206C", "uni206D", "uni206E", "uni206F",
            "uni3164", "uniF8FF", "uni101DC8",
        }
        if not preserve_arabic_joining:
            problematic.update({"uni200C", "uni200D", "uni200E", "uni200F"})

        to_remove = []
        for g in font.glyphs():
            try:
                if g.glyphname in problematic:
                    to_remove.append(g.glyphname)
                    continue
                uni = g.unicode
                if uni != -1 and uni not in ESSENTIAL_GLYPHS:
                    if uni > 0xFFFF:
                        to_remove.append(g.glyphname)
            except Exception:
                pass

        for gname in to_remove:
            try:
                font.removeGlyph(gname)
            except Exception:
                pass
    except Exception as e:
        print(f"     -> Warning during glyph cleanup: {e}")


def _subtract_ranges(base_ranges, keep_ranges):
    spans = list(base_ranges)
    for ks, ke in keep_ranges:
        next_spans = []
        for bs, be in spans:
            if ke < bs or ks > be:
                next_spans.append((bs, be))
            else:
                if bs < ks:
                    next_spans.append((bs, ks - 1))
                if be > ke:
                    next_spans.append((ke + 1, be))
        spans = next_spans
    return spans


def _font_has_range(font, start, end):
    for cp in range(int(start), int(end) + 1):
        if cp in font:
            return True
    return False


def _prepare_source_font(
    path,
    target_metrics,
    mode="visual",
    wipe_latin=False,
    sync_symbols_only=False,
    preserve_arabic_joining=True,
    remove_kern_lookups=False,
):
    """Opens a source font in memory, normalizes grid/scale if visual mode, and returns it."""
    if not path or path == "NONE" or not os.path.exists(path):
        return None

    font = None
    try:
        font = fontforge.open(path)

        if mode == "visual":
            try:
                font.em = target_metrics["em"]
                font.ascent = target_metrics["ascent"]
                font.descent = target_metrics["descent"]

                ref_code = 0
                ref_target = 0
                if 0x48 in font:
                    ref_code = 0x48
                    ref_target = target_metrics["lat_ref_h"]
                elif 0x0627 in font:
                    ref_code = 0x0627
                    ref_target = target_metrics["ara_ref_h"]

                if ref_code > 0:
                    src_h = _glyph_bbox_height(font, ref_code)
                    if src_h > 0:
                        scale = ref_target / src_h
                        print(
                            f"     -> Visual Normalization: {src_h:.1f} -> {ref_target:.1f} (factor {scale:.4f})"
                        )
                        if abs(scale - 1.0) > 0.005:
                            # font.transform(psMat.scale(...)) scales both outlines and advance widths
                            font.selection.all()
                            font.transform(psMat.scale(scale, scale))
            except Exception as e:
                print(f"     -> Warning normalizing grid/EM: {e}")

        print(f"     -> Cleaning up glyphs in {os.path.basename(path)}...")
        _cleanup_unused_glyphs(font, preserve_arabic_joining, remove_kern_lookups)

        if wipe_latin:
            try:
                spans = _subtract_ranges([(0x0020, 0x024F), (0x1E00, 0x1EFF)], EXTRA_RANGES)
                if spans:
                    font.selection.select(("ranges",), spans[0][0], spans[0][1])
                    for s, e in spans[1:]:
                        font.selection.select(("more", "ranges"), s, e)
                    font.clear()
            except Exception as e:
                print(f"     -> Error clearing Latin range: {e}", file=sys.stderr)

        arabic_requested = any(s <= 0x06FF and e >= 0x0600 for s, e in EXTRA_RANGES)
        if sync_symbols_only and not arabic_requested:
            try:
                font.selection.select(("ranges",), 0x0600, 0x06FF)
                font.selection.select(("more", "ranges"), 0x0750, 0x077F)
                font.selection.select(("more", "ranges"), 0x08A0, 0x08FF)
                font.selection.select(("more", "ranges"), 0xFB50, 0xFDFF)
                font.selection.select(("more", "ranges"), 0xFE70, 0xFEFF)
                font.clear()
            except Exception as e:
                print(f"     -> Error clearing Arabic leftovers: {e}", file=sys.stderr)

        return font
    except Exception as e:
        print(f"     -> Critical error processing {os.path.basename(path)}: {e}", file=sys.stderr)
        if font:
            try:
                font.close()
            except Exception:
                pass
        return None


def _prepare_segoe_fallback(segoe_font, is_latin_only, active_fonts):
    """Clears replaced ranges from the already-open Segoe UI font in memory."""
    segoe_font.selection.select(("ranges",), 0x0020, 0x024F)
    segoe_font.selection.select(("more", "ranges"), 0x1E00, 0x1EFF)
    for s, e in EXTRA_RANGES:
        if any(_font_has_range(f, s, e) for f in active_fonts):
            segoe_font.selection.select(("more", "ranges"), s, e)
    segoe_font.clear()

    if not is_latin_only:
        for lookup in list(segoe_font.gsub_lookups):
            try:
                segoe_font.removeLookup(lookup)
            except Exception:
                pass
        for lookup in list(segoe_font.gpos_lookups):
            try:
                segoe_font.removeLookup(lookup)
            except Exception:
                pass

    _cleanup_unused_glyphs(segoe_font, preserve_arabic_joining=True, remove_kern_lookups=False)


def process_weight(latin_path, arabic_path, weight_type, segoe_filename, mode="visual", out_dir=None):
    if latin_path == "NONE":
        print(f"  -> Error: Missing Latin input for {weight_type}.", file=sys.stderr)
        return False

    base_dir = out_dir or os.getcwd()
    print(f"\n[Engine] Processing {weight_type} weight...")
    windir = os.environ.get("SEGOEUI_WINDIR") or os.environ.get("WINDIR") or r"C:\Windows"
    segoe_path = os.path.join(windir, "Fonts", segoe_filename)
    if not os.path.exists(segoe_path):
        print(f"  -> Error: System {segoe_filename} not found at {segoe_path}.", file=sys.stderr)
        return False

    segoe_font = None
    lat_font = None
    ara_font = None
    extra_fonts = []
    final_font = None

    try:
        segoe_font = fontforge.open(segoe_path)
        segoe_info = _extract_segoe_info(segoe_font)
        is_latin_only = arabic_path == "NONE" and not EXTRA_FONT_MAP.get(weight_type)
        print(f"  -> Syncing to Segoe standard grid ({segoe_info['em']} EM) [{mode}]")

        lat_font = _prepare_source_font(
            latin_path,
            segoe_info,
            mode=mode,
            sync_symbols_only=is_latin_only,
            preserve_arabic_joining=True,
            remove_kern_lookups=False,
        )
        if not lat_font:
            print(f"  -> Error: Failed to prepare Latin font. Skipping {weight_type}...", file=sys.stderr)
            return False

        if arabic_path != "NONE":
            ara_font = _prepare_source_font(
                arabic_path,
                segoe_info,
                mode=mode,
                wipe_latin=True,
                preserve_arabic_joining=True,
                remove_kern_lookups=False,
            )
            if not ara_font:
                print(f"  -> Error: Failed to prepare Secondary font. Skipping {weight_type}...", file=sys.stderr)
                return False

        for extra_path in EXTRA_FONT_MAP.get(weight_type, []):
            if not extra_path or extra_path == "NONE":
                continue
            ef = _prepare_source_font(
                extra_path,
                segoe_info,
                mode=mode,
                wipe_latin=True,
                preserve_arabic_joining=True,
                remove_kern_lookups=False,
            )
            if ef is None:
                print(f"  -> Error: Failed to prepare extra font '{extra_path}' for {weight_type}.", file=sys.stderr)
                return False
            extra_fonts.append(ef)

        active_fonts = [f for f in [lat_font, ara_font, *extra_fonts] if f is not None]
        _prepare_segoe_fallback(segoe_font, is_latin_only, active_fonts)

        print("  -> Assembling final font...")
        if ara_font is not None:
            final_font = ara_font
            ara_font = None
            final_font.mergeFonts(lat_font)
        else:
            final_font = lat_font
            lat_font = None

        for ef in extra_fonts:
            final_font.mergeFonts(ef)

        final_font.mergeFonts(segoe_font)
        _apply_segoe_metadata(final_font, segoe_info)

        output_name = os.path.join(base_dir, f"{segoe_filename.split('.')[0]}_system_mod.ttf")
        final_font.generate(output_name)

        print(f"  -> Success! Saved as: {os.path.basename(output_name)}")
        return True
    except Exception as e:
        print(f"  -> Unexpected error processing {weight_type}: {e}", file=sys.stderr)
        return False
    finally:
        # Run GC before closing fonts so FontForge Glyph wrappers don't finalize after font.close()
        gc.collect()
        for obj in [final_font, lat_font, ara_font, segoe_font, *extra_fonts]:
            if obj is not None:
                try:
                    obj.close()
                except Exception:
                    pass
        gc.collect()


def run_batch(args, style="normal", default_mode="visual"):
    mode = args[12].lower() if len(args) > 12 else default_mode
    weights = ITALIC_WEIGHTS if style == "italic" else NORMAL_WEIGHTS
    label = "Italic" if style == "italic" else "Standard"
    print(f"\n[Engine] Starting In-Memory {label} Builder ({mode} mode)...")
    all_ok = True
    for idx, (weight_name, sys_filename) in enumerate(weights):
        if args[idx] == "NONE":
            continue
        if not process_weight(args[idx], args[6 + idx], weight_name, sys_filename, mode=mode):
            all_ok = False
    if not all_ok:
        print("\n[Engine] Build finished with errors.", file=sys.stderr)
        sys.exit(1)
    print("\n[Engine] All system replacement fonts built successfully!")


if __name__ == "__main__":
    argv = sys.argv[1:]
    if len(argv) >= 6 and argv[0] == "--single":
        # --single <weight_name> <lat_path> <ara_path> <segoe_filename> <mode>
        ok = process_weight(argv[2], argv[3], argv[1], argv[4], mode=argv[5].lower())
        sys.exit(0 if ok else 1)
    elif len(argv) >= 12:
        run_batch(argv)
    else:
        print("Usage: engine_core.py --single <weight> <lat> <ara> <segoe_ttf> <mode>", file=sys.stderr)
        sys.exit(2)
