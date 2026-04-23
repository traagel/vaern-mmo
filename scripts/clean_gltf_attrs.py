#!/usr/bin/env python3
"""Silence Bevy's `Unknown vertex attribute` warnings by stripping attributes
it doesn't support, and fix 8-bone skinning that would otherwise drop
influences.

Two passes per .gltf under `assets/extracted/`:

  Unsupported attributes → stripped
    Bevy 0.18 only reads POSITION, NORMAL, TANGENT, TEXCOORD_0, TEXCOORD_1,
    COLOR_0, JOINTS_0, WEIGHTS_0. Anything else (COLOR_N≥1, TEXCOORD_N≥2,
    custom channels from DCC tools) is removed from each primitive's
    `attributes` dict. Underlying accessors + bufferViews stay in place
    (harmless orphans).

  JOINTS_1 / WEIGHTS_1 → merged, then stripped
    Special case: these carry the 5th-8th skinning influences, which Bevy
    silently drops. Plain strip would lose data. Instead: pool all 8
    influences per vertex, pick the top 4 by weight, renormalize, rewrite
    JOINTS_0 / WEIGHTS_0 bytes in-place in the .bin file. Then remove
    JOINTS_1 / WEIGHTS_1 from the attributes dict.

Idempotent: re-running after a clean pass does nothing.
"""

import json
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
EXTRACTED = HERE.parent / "assets/extracted"

# Attributes Bevy 0.18 actually reads. Anything else is dead weight.
SUPPORTED_ATTRS = frozenset({
    "POSITION",
    "NORMAL",
    "TANGENT",
    "TEXCOORD_0",
    "TEXCOORD_1",
    "COLOR_0",
    "JOINTS_0",
    "WEIGHTS_0",
})

# These need merge-then-strip, not plain strip — dropping them loses skinning.
MERGE_ATTRS = frozenset({"JOINTS_1", "WEIGHTS_1"})

COMPONENT_DTYPES = {
    5120: np.int8,
    5121: np.uint8,
    5122: np.int16,
    5123: np.uint16,
    5125: np.uint32,
    5126: np.float32,
}
TYPE_COMPONENTS = {
    "SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4,
    "MAT2":   4, "MAT3": 9, "MAT4": 16,
}


def accessor_slice(gltf, acc_idx):
    """Return (byte_offset, byte_length, dtype, components) for an accessor."""
    acc = gltf["accessors"][acc_idx]
    view = gltf["bufferViews"][acc["bufferView"]]
    offset = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    dtype = COMPONENT_DTYPES[acc["componentType"]]
    comps = TYPE_COMPONENTS[acc["type"]]
    length = acc["count"] * comps * np.dtype(dtype).itemsize
    return offset, length, dtype, comps, acc.get("normalized", False)


def read_vec(buf, gltf, acc_idx):
    offset, length, dtype, comps, normalized = accessor_slice(gltf, acc_idx)
    arr = np.frombuffer(buf[offset : offset + length], dtype=dtype).reshape(-1, comps).copy()
    if normalized and np.issubdtype(dtype, np.integer):
        max_val = np.iinfo(dtype).max
        return arr.astype(np.float32) / max_val, dtype, normalized, offset, length
    return arr, dtype, normalized, offset, length


def merge_to_top4(joints_0, joints_1, weights_0_f, weights_1_f):
    """Collapse 8-bone skinning to top-4 per vertex. Returns (joints4, weights4)
    with weights renormalized to sum to 1.
    """
    joints_all = np.hstack([joints_0, joints_1])       # (N, 8)
    weights_all = np.hstack([weights_0_f, weights_1_f])  # (N, 8) float
    top4 = np.argsort(-weights_all, axis=1)[:, :4]     # indices of top 4
    rows = np.arange(joints_all.shape[0])[:, None]
    j4 = joints_all[rows, top4]
    w4 = weights_all[rows, top4]
    s = w4.sum(axis=1, keepdims=True)
    s[s == 0] = 1.0  # avoid div-by-zero; zero-weight vertices stay zero
    w4 = w4 / s
    return j4, w4


def write_vec(buf, gltf, acc_idx, arr, original_dtype, normalized):
    """Overwrite the bytes backing an accessor with new data. `arr` is float
    for normalized accessors; we convert back to the original integer dtype.
    Shape must match the accessor's count × components.
    """
    offset, length, _, _, _ = accessor_slice(gltf, acc_idx)
    if normalized and np.issubdtype(original_dtype, np.integer):
        max_val = np.iinfo(original_dtype).max
        packed = np.clip(np.round(arr * max_val), 0, max_val).astype(original_dtype)
    else:
        packed = arr.astype(original_dtype)
    out = packed.tobytes()
    assert len(out) == length, f"size mismatch: {len(out)} vs {length}"
    buf[offset : offset + length] = out


def process_file(gltf_path: Path):
    gltf = json.loads(gltf_path.read_text())
    buffers = gltf.get("buffers", [])
    if not buffers:
        return False, "no buffers"

    # Load the sole buffer. Multi-buffer glTFs exist but this pack doesn't
    # use them — fall back to error if surprised.
    if len(buffers) > 1:
        return False, f"multi-buffer ({len(buffers)}) — skipping"
    buf_path = gltf_path.parent / buffers[0]["uri"]
    buf = bytearray(buf_path.read_bytes())

    changed_json = False
    changed_bin = False
    stripped_counts: dict[str, int] = {}
    joints_merged = 0

    for m in gltf.get("meshes", []):
        for p in m.get("primitives", []):
            attrs = p.get("attributes", {})

            # Merge 8-bone skinning into top-4 before stripping the extras.
            if "JOINTS_1" in attrs and "WEIGHTS_1" in attrs:
                j0_acc = attrs["JOINTS_0"]
                j1_acc = attrs["JOINTS_1"]
                w0_acc = attrs["WEIGHTS_0"]
                w1_acc = attrs["WEIGHTS_1"]

                j0, j0_dt, _,          _, _ = read_vec(buf, gltf, j0_acc)
                j1, j1_dt, _,          _, _ = read_vec(buf, gltf, j1_acc)
                w0_f, w0_dt, w0_norm,  _, _ = read_vec(buf, gltf, w0_acc)
                w1_f, w1_dt, w1_norm,  _, _ = read_vec(buf, gltf, w1_acc)

                j4, w4 = merge_to_top4(j0, j1, w0_f, w1_f)

                write_vec(buf, gltf, j0_acc, j4, j0_dt, False)
                write_vec(buf, gltf, w0_acc, w4, w0_dt, w0_norm)
                joints_merged += 1
                changed_bin = True
                # JOINTS_1/WEIGHTS_1 will be removed by the generic strip below.

            # Generic strip: remove every attribute Bevy doesn't read. Accessors
            # + bufferViews stay behind as unreferenced orphans — glTF permits.
            unsupported = [
                k for k in attrs.keys()
                if k not in SUPPORTED_ATTRS
            ]
            for k in unsupported:
                del attrs[k]
                stripped_counts[k] = stripped_counts.get(k, 0) + 1
                changed_json = True

    if changed_json:
        gltf_path.write_text(json.dumps(gltf, indent=2))
    if changed_bin:
        buf_path.write_bytes(bytes(buf))

    if stripped_counts or joints_merged:
        parts = []
        if joints_merged:
            parts.append(f"merged {joints_merged} 8-bone primitive(s)")
        if stripped_counts:
            summary = ", ".join(f"{k}×{v}" for k, v in sorted(stripped_counts.items()))
            parts.append(f"stripped {summary}")
        return True, "; ".join(parts)
    return False, "clean"


def main():
    if not EXTRACTED.is_dir():
        print(f"error: {EXTRACTED} does not exist", file=sys.stderr)
        sys.exit(1)

    total = 0
    touched = 0
    for gltf_path in sorted(EXTRACTED.rglob("*.gltf")):
        total += 1
        changed, note = process_file(gltf_path)
        if changed:
            touched += 1
            rel = gltf_path.relative_to(EXTRACTED)
            print(f"  {rel}: {note}")
    print(f"\nscanned {total} .gltf files, modified {touched}")


if __name__ == "__main__":
    main()
