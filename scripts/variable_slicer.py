import io
import json
import os
import shutil
import sys
from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib.instancer import instantiateVariableFont


def _load_ref_unicodes(extra_ranges=None) -> set[int]:
    """Returns Segoe UI + selected script Unicode codepoints to retain during pruning."""
    if not extra_ranges:
        extra_ranges = [(0x1780, 0x17FF), (0x19E0, 0x19FF)]
    ref_unicodes = set()
    for start, end in extra_ranges:
        ref_unicodes |= set(range(int(start), int(end) + 1))
    windir = os.environ.get("WINDIR", r"C:\Windows")
    segoe_ref = os.path.join(windir, "Fonts", "segoeui.ttf")
    if os.path.exists(segoe_ref):
        ref_font = TTFont(segoe_ref, lazy=True)
        cmap = ref_font.getBestCmap()
        if cmap:
            ref_unicodes |= set(cmap.keys())
        ref_font.close()
    return ref_unicodes


def _prune_variable_font_bytes(var_path: str, ref_unicodes: set[int]) -> bytes:
    """Prunes a variable font once before instancing weights (major speedup)."""
    font = TTFont(var_path)
    try:
        static_cmap = font.getBestCmap() or {}
        glyph_order = set(font.getGlyphOrder())
        glyph_set = font.getGlyphSet()
        valid_unicodes = [
            u
            for u in ref_unicodes
            if u in static_cmap
            and static_cmap[u] in glyph_order
            and static_cmap[u] in glyph_set
        ]
        if valid_unicodes:
            options = subset.Options()
            options.layout_features = ["*"]
            subsetter = subset.Subsetter(options=options)
            subsetter.populate(unicodes=valid_unicodes)
            subsetter.subset(font)
    except Exception as e:
        print(f"[Slicer] Pre-pruning skipped (non-critical): {e}", flush=True)

    buf = io.BytesIO()
    font.save(buf)
    font.close()
    return buf.getvalue()


def slice_variable_font(
    var_path: str,
    weight_val: int,
    out_path: str,
    pruned_bytes: bytes | None = None,
) -> None:
    """Instantiates a static weight from a pre-pruned variable font."""
    if pruned_bytes is not None:
        font = TTFont(io.BytesIO(pruned_bytes))
    else:
        pruned = _prune_variable_font_bytes(var_path, _load_ref_unicodes())
        font = TTFont(io.BytesIO(pruned))

    # Pin all variable axes (e.g. opsz, GRAD, wdth) to defaultValue so output is 100% static
    axis_locs = {"wght": weight_val}
    if "fvar" in font:
        axis_locs = {a.axisTag: a.defaultValue for a in font["fvar"].axes}
        if "wght" in axis_locs:
            axis_locs["wght"] = weight_val

    # inplace=True avoids expensive deepcopy inside fontTools; font is a fresh per-weight buffer
    instantiateVariableFont(font, axis_locs, inplace=True)
    font.save(out_path)
    font.close()


def create_variable_spoof(input_path: str, output_path: str) -> None:
    """Creates SegUIVar_system_mod.ttf with its name table patched to 'Segoe UI Variable'."""
    try:
        font = TTFont(input_path)
        if "name" in font:
            for record in font["name"].names:
                if record.nameID in (1, 4, 6, 16):
                    val = "SegoeUIVariable" if record.nameID == 6 else "Segoe UI Variable"
                    try:
                        record.string = val.encode(record.getEncoding())
                    except Exception:
                        pass
        font.save(output_path)
        font.close()
    except Exception:
        shutil.copy(input_path, output_path)


def resolve_weights(
    base_path: str,
    is_var: bool,
    reg_path: str,
    light_path: str,
    semilight_path: str,
    semibold_path: str,
    bold_path: str,
    black_path: str,
    lang_prefix: str,
    weights=None,
    extra_ranges=None,
):
    if weights is None:
        weights = [300, 350, 400, 600, 700, 900]

    paths = []
    if is_var and reg_path and reg_path != "NONE":
        total = len(weights)
        print(f"SLICING_PROGRESS 0/{total} {lang_prefix} pruning", flush=True)
        pruned_bytes = _prune_variable_font_bytes(
            reg_path, _load_ref_unicodes(extra_ranges)
        )
        for idx, w in enumerate(weights, start=1):
            print(f"SLICING_PROGRESS {idx}/{total} {lang_prefix} {w}", flush=True)
            out = os.path.join(base_path, f"temp_{lang_prefix}_{w}.ttf")
            slice_variable_font(reg_path, w, out, pruned_bytes=pruned_bytes)
            paths.append(out)
    else:
        r = reg_path if reg_path else "NONE"
        lt = light_path if light_path else r
        sl = semilight_path if semilight_path else r
        b = bold_path if bold_path else r
        sb = semibold_path if semibold_path else b
        blk = black_path if black_path else b
        static_map = [lt, sl, r, sb, b, blk]
        paths = static_map[: len(weights)]
    return paths


if __name__ == "__main__":
    if len(sys.argv) < 2:
        sys.exit(1)
    if sys.argv[1] == "--spoof" and len(sys.argv) >= 4:
        create_variable_spoof(sys.argv[2], sys.argv[3])
        sys.exit(0)
    with open(sys.argv[1], "r", encoding="utf-8-sig") as f:
        req = json.load(f)
    res = resolve_weights(
        req["base_path"],
        req["is_var"],
        req["reg"],
        req["light"],
        req["semilight"],
        req["semibold"],
        req["bold"],
        req["black"],
        req["prefix"],
        extra_ranges=req.get("extra_ranges"),
    )
    print(f"SLICER_RESULT {json.dumps(res)}", flush=True)
