#!/usr/bin/env python3
"""Split the Infinigon Superhero base-character mesh into per-region sub-meshes
via skin-weight classification, so the museum (and eventually the real gear
system) can hide individual body regions while keeping the head visible.

Input:  assets/extracted/characters/base/Superhero_{Male,Female}_FullBody.{gltf,bin}
Output: assets/extracted/characters/base/Superhero_{Male,Female}_FullBody_Split.{gltf,bin}

Regions (6): Head, Torso, LeftArm, RightArm, LeftLeg, RightLeg.

How it works:
  1. Parse glTF + .bin. Pull JOINTS_0 / WEIGHTS_0 per vertex + index buffer.
  2. For each vertex, sum the 4 weighted joints into per-region weight buckets
     and tag it with its dominant region.
  3. For each triangle, pick the region whose summed weight across the 3
     vertices is highest (majority by influence, not vertex count — survives
     near-boundary edge cases better).
  4. Emit one extra mesh per non-empty region, each referencing the same
     vertex attributes but with its own filtered index buffer appended to
     a new .bin. Rewire the scene graph: replace the original body node with
     6 region-named nodes (all skinned to the same 65-joint armature).
  5. The original body mesh stays in the glTF meshes array unreferenced —
     glTF tolerates dead meshes; consumers only render what's reachable.
"""

import json
import sys
from pathlib import Path

import numpy as np

HERE = Path(__file__).resolve().parent
BASE_DIR = HERE.parent / "assets/extracted/characters/base"

# Universal skeleton layout (verified by inspecting the glTF joint list).
# Each region is a contiguous range of joint indices in the skin's joint array.
REGION_JOINTS = {
    "Head":     [5, 6],                  # neck_01, Head
    "Torso":    [0, 1, 2, 3, 4],         # root, pelvis, spine_01-03
    "LeftArm":  list(range(7, 31)),      # clavicle_l → thumb_04_leaf_l (24 joints)
    "RightArm": list(range(31, 55)),     # clavicle_r → thumb_04_leaf_r (24 joints)
    "LeftLeg":  list(range(55, 60)),     # thigh_l → ball_leaf_l
    "RightLeg": list(range(60, 65)),     # thigh_r → ball_leaf_r
}
REGION_ORDER = ["Head", "Torso", "LeftArm", "RightArm", "LeftLeg", "RightLeg"]
REGION_INDEX = {r: i for i, r in enumerate(REGION_ORDER)}

BODY_MESH_NAMES = {
    "Male":   "Sphere.005_Retopology.004",
    "Female": "Superhero_Female",
}
BODY_NODE_NAMES = {
    "Male":   "SuperHero_Male",
    "Female": "Superhero_Female",
}

# glTF component type enums.
COMPONENT_DTYPES = {
    5120: np.int8,     # BYTE
    5121: np.uint8,    # UNSIGNED_BYTE
    5122: np.int16,    # SHORT
    5123: np.uint16,   # UNSIGNED_SHORT
    5125: np.uint32,   # UNSIGNED_INT
    5126: np.float32,  # FLOAT
}
TYPE_COMPONENTS = {
    "SCALAR": 1, "VEC2": 2, "VEC3": 3, "VEC4": 4,
    "MAT2":   4, "MAT3": 9, "MAT4": 16,
}


def read_accessor(gltf, buf_bytes, acc_idx):
    acc = gltf["accessors"][acc_idx]
    view = gltf["bufferViews"][acc["bufferView"]]
    offset = view.get("byteOffset", 0) + acc.get("byteOffset", 0)
    count = acc["count"]
    dtype = COMPONENT_DTYPES[acc["componentType"]]
    comps = TYPE_COMPONENTS[acc["type"]]
    n_bytes = count * comps * np.dtype(dtype).itemsize
    arr = np.frombuffer(buf_bytes[offset : offset + n_bytes], dtype=dtype).copy()
    return arr.reshape(count, comps) if comps > 1 else arr


def classify_triangles(joints, weights, indices):
    """Assign each triangle to a region based on the region with the highest
    total skin-weight across its 3 vertices.
    """
    n_verts = joints.shape[0]
    region_scores = np.zeros((n_verts, len(REGION_ORDER)), dtype=np.float32)

    # For each of the 4 influence slots per vertex, look up which region
    # the joint belongs to and accumulate the slot's weight there.
    for slot in range(4):
        j_slot = joints[:, slot].astype(np.int32)
        w_slot = weights[:, slot].astype(np.float32)
        # Normalize uint8 weights to [0,1] if they look unnormalized.
        if weights.dtype == np.uint8:
            w_slot = w_slot / 255.0
        elif weights.dtype == np.uint16:
            w_slot = w_slot / 65535.0
        for region, joint_list in REGION_JOINTS.items():
            mask = np.isin(j_slot, joint_list)
            region_scores[mask, REGION_INDEX[region]] += w_slot[mask]

    tri_count = indices.size // 3
    triangles = indices.reshape(tri_count, 3).astype(np.int64)
    # Triangle score = sum of vertex region scores — picks the region with
    # the strongest total pull across all 3 verts.
    tri_scores = (
        region_scores[triangles[:, 0]]
        + region_scores[triangles[:, 1]]
        + region_scores[triangles[:, 2]]
    )
    tri_regions = np.argmax(tri_scores, axis=1)

    return {r: triangles[tri_regions == REGION_INDEX[r]].flatten()
            for r in REGION_ORDER}


def find_mesh_by_name(gltf, name):
    for i, m in enumerate(gltf["meshes"]):
        if m.get("name") == name:
            return i
    raise KeyError(f"no mesh named {name!r}")


def find_node_by_name(gltf, name):
    for i, n in enumerate(gltf["nodes"]):
        if n.get("name") == name:
            return i
    raise KeyError(f"no node named {name!r}")


def align_to(buf, alignment):
    while len(buf) % alignment != 0:
        buf.append(0)


def split_character(gender: str):
    src_name = f"Superhero_{gender}_FullBody"
    src_gltf_path = BASE_DIR / f"{src_name}.gltf"
    src_bin_path = BASE_DIR / f"{src_name}.bin"
    dst_name = f"{src_name}_Split"
    dst_gltf_path = BASE_DIR / f"{dst_name}.gltf"
    dst_bin_path = BASE_DIR / f"{dst_name}.bin"

    if not src_gltf_path.exists():
        print(f"skip {gender}: {src_gltf_path} missing")
        return

    print(f"\n=== {gender} ===")
    print(f"  reading  {src_gltf_path.name}")

    gltf = json.loads(src_gltf_path.read_text())
    src_bin = src_bin_path.read_bytes()

    body_mesh_name = BODY_MESH_NAMES[gender]
    body_node_name = BODY_NODE_NAMES[gender]
    body_mesh_idx = find_mesh_by_name(gltf, body_mesh_name)
    body_node_idx = find_node_by_name(gltf, body_node_name)
    body_mesh = gltf["meshes"][body_mesh_idx]
    assert len(body_mesh["primitives"]) == 1, \
        f"body mesh has {len(body_mesh['primitives'])} primitives, expected 1"
    body_prim = body_mesh["primitives"][0]

    attrs = body_prim["attributes"]
    joints = read_accessor(gltf, src_bin, attrs["JOINTS_0"])
    weights = read_accessor(gltf, src_bin, attrs["WEIGHTS_0"])
    indices = read_accessor(gltf, src_bin, body_prim["indices"])

    n_verts = joints.shape[0]
    n_tris = indices.size // 3
    print(f"  body mesh[{body_mesh_idx}]: {n_verts} verts, {n_tris} triangles")

    region_tris = classify_triangles(joints, weights, indices)

    # Extend the binary buffer with one uint32 index array per region.
    new_bin = bytearray(src_bin)
    align_to(new_bin, 4)

    new_node_idxs = []
    for region in REGION_ORDER:
        tris = region_tris[region]
        if tris.size == 0:
            print(f"    {region:9}: (empty)")
            continue

        view_offset = len(new_bin)
        payload = tris.astype("<u4").tobytes()
        new_bin.extend(payload)
        align_to(new_bin, 4)

        bv_idx = len(gltf["bufferViews"])
        gltf["bufferViews"].append({
            "buffer": 0,
            "byteOffset": view_offset,
            "byteLength": len(payload),
            "target": 34963,  # ELEMENT_ARRAY_BUFFER
        })

        acc_idx = len(gltf["accessors"])
        gltf["accessors"].append({
            "bufferView":    bv_idx,
            "componentType": 5125,       # UNSIGNED_INT
            "count":         int(tris.size),
            "type":          "SCALAR",
        })

        # New primitive copies the vertex attributes + material from the
        # original body primitive but swaps in the filtered index buffer.
        new_prim = {
            "attributes": dict(attrs),
            "indices":    acc_idx,
        }
        if "material" in body_prim:
            new_prim["material"] = body_prim["material"]
        if "mode" in body_prim:
            new_prim["mode"] = body_prim["mode"]

        mesh_idx = len(gltf["meshes"])
        gltf["meshes"].append({
            "name":       f"Region_{region}",
            "primitives": [new_prim],
        })

        node_idx = len(gltf["nodes"])
        gltf["nodes"].append({
            "name": f"Region_{region}",
            "mesh": mesh_idx,
            "skin": gltf["nodes"][body_node_idx].get("skin", 0),
        })
        new_node_idxs.append(node_idx)

        print(f"    {region:9}: mesh[{mesh_idx}] node[{node_idx}] — {tris.size // 3} tris")

    # Splice the new region nodes into the scene graph where the original
    # body node sat. Try "parent with `children`" first, fall back to root.
    parent_idx = None
    for i, n in enumerate(gltf["nodes"]):
        if "children" in n and body_node_idx in n["children"]:
            parent_idx = i
            break

    if parent_idx is not None:
        parent = gltf["nodes"][parent_idx]
        parent["children"] = [c for c in parent["children"] if c != body_node_idx] + new_node_idxs
        print(f"  replaced body node [{body_node_idx}] in parent [{parent_idx}]'s children")
    else:
        for scene in gltf.get("scenes", []):
            if body_node_idx in scene.get("nodes", []):
                scene["nodes"] = [n for n in scene["nodes"] if n != body_node_idx] + new_node_idxs
                print(f"  replaced body node [{body_node_idx}] in scene roots")
                break
        else:
            print(f"  WARN: body node [{body_node_idx}] not found in any scene graph!")

    # Point buffer 0 at the new .bin and update its length.
    gltf["buffers"][0]["uri"] = f"{dst_name}.bin"
    gltf["buffers"][0]["byteLength"] = len(new_bin)

    print(f"  writing  {dst_gltf_path.name} ({len(new_bin)} bytes in {dst_bin_path.name})")
    dst_gltf_path.write_text(json.dumps(gltf, indent=2))
    dst_bin_path.write_bytes(bytes(new_bin))


def main():
    if not BASE_DIR.is_dir():
        print(f"error: {BASE_DIR} does not exist", file=sys.stderr)
        sys.exit(1)
    for gender in ("Male", "Female"):
        split_character(gender)


if __name__ == "__main__":
    main()
