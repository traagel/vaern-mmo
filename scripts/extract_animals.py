#!/usr/bin/env python3
"""Extract the EverythingLibrary Animals pack into one GLB per species.

The shipped archive contains:
  - EverythingLibrary_Animals_001.fbx  (every species in one file)
  - EverythingLibrary_Animals_002.blend (cleanly organized source scene)

The .blend is far better organized — there's a single root EMPTY
(`ANIMALS`) with 10 category children (`Animals`, `Birds`, `Reptiles`,
`Imaginary`, …), each containing per-species EMPTY nodes whose
descendants are the species meshes. We open the .blend in Blender's
headless CLI mode, walk that tree, and export each species empty +
its mesh children as a standalone GLB.

No skinning / no animation — every species is a static mesh. Output
layout mirrors `assets/extracted/props/`:
  assets/extracted/animals/
      GrayWolf.glb
      Deer.glb
      ...

Usage:
  blender --background --python scripts/extract_animals.py
"""
import sys
from pathlib import Path

try:
    import bpy
except ImportError:
    sys.stderr.write(
        "extract_animals.py must run inside Blender: "
        "`blender --background --python scripts/extract_animals.py`\n"
    )
    sys.exit(1)


REPO = Path(__file__).resolve().parent.parent
ZIP_PATH = REPO / "assets/zips/EverythingLibrary_Animals_002.zip"
OUT_DIR = REPO / "assets/extracted/animals"
# Cache extracted .blend next to the zip so repeat runs skip the unzip.
CACHE_DIR = REPO / "assets/extracted/_animals_raw"
BLEND_NAME = "EverythingLibrary_Animals_002.blend"


def ensure_blend() -> Path:
    """Unzip the archive into the cache dir on first run; return the .blend path."""
    blend = CACHE_DIR / BLEND_NAME
    if blend.is_file():
        return blend
    import zipfile

    CACHE_DIR.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(ZIP_PATH, "r") as zf:
        zf.extractall(CACHE_DIR)
    if not blend.is_file():
        sys.exit(f"expected {blend} after unzip; got {list(CACHE_DIR.iterdir())}")
    return blend


def select_with_descendants(obj) -> None:
    """Select `obj` and every descendant in the scene hierarchy."""
    obj.select_set(True)
    for child in obj.children_recursive:
        child.select_set(True)


def alt_variant_pair(empty, category: str) -> tuple | None:
    """If `empty` is a non-AnimalParts species with exactly two
    `alt…` mesh descendants — the male/female or rearing/grounded
    variant pairs — return them sorted alphabetically as
    `(primary, secondary)`. Otherwise `None`.

    Variants get exported as two separate GLBs (`<Species>.glb` and
    `<Species>_b.glb`) so npc_mesh_map.yaml can pick whichever pose
    looks right per-mob. Without that split, exporting both into one
    GLB stacks the two bodies; picking just one (the old behavior)
    forces every mob to wear the same variant forever."""
    if category == "AnimalParts":
        return None
    alt_meshes = [
        d
        for d in empty.children_recursive
        if d.type == "MESH" and d.name.startswith("alt")
    ]
    if len(alt_meshes) != 2:
        return None
    alt_meshes.sort(key=lambda m: m.name)
    return alt_meshes[0], alt_meshes[1]


def deselect_meshes(meshes) -> None:
    for m in meshes:
        m.select_set(False)


def export_species(name: str, empty, category: str = "") -> bool:
    """Export `empty` + its mesh-carrying descendants. When `empty`
    has a variant pair (e.g. four-legged vs rearing bear), the primary
    lands at `<name>.glb` and the secondary at `<name>_b.glb`.
    Returns True if at least one GLB was written."""
    meshes = [d for d in empty.children_recursive if d.type == "MESH"]
    if not meshes:
        return False

    variants = alt_variant_pair(empty, category)

    if variants is None:
        bpy.ops.object.select_all(action="DESELECT")
        select_with_descendants(empty)
        bpy.context.view_layer.objects.active = empty
        _write_glb(name, empty)
        return True

    # Two variants — export each in turn. Selection is reset between
    # passes so primary doesn't carry secondary's mesh.
    primary, secondary = variants
    # Primary: select everything except the secondary variant.
    bpy.ops.object.select_all(action="DESELECT")
    select_with_descendants(empty)
    deselect_meshes([secondary])
    bpy.context.view_layer.objects.active = empty
    _write_glb(name, empty)
    # Secondary: same, but swap which variant stays selected.
    bpy.ops.object.select_all(action="DESELECT")
    select_with_descendants(empty)
    deselect_meshes([primary])
    bpy.context.view_layer.objects.active = empty
    _write_glb(f"{name}_b", empty)
    return True


def _write_glb(name: str, empty) -> None:
    """Zero the species parent's transform, run the glTF export,
    then restore the transform. Shared between the single-variant
    and variant-pair paths."""

    # Each species sits at its own stage position inside the source
    # scene (the library is laid out in a big grid). Zero the
    # parent's transform before export so every GLB lands at origin,
    # then restore it so other species aren't disturbed.
    saved_loc = tuple(empty.location)
    saved_rot = tuple(empty.rotation_euler)
    empty.location = (0.0, 0.0, 0.0)
    empty.rotation_euler = (0.0, 0.0, 0.0)

    out = OUT_DIR / f"{name}.glb"
    bpy.ops.export_scene.gltf(
        filepath=str(out),
        export_format="GLB",
        use_selection=True,
        # Critical: without `use_active_scene=True` Blender's glTF
        # exporter iterates every scene in the .blend, so every
        # species GLB ends up carrying the entire library (1689
        # nodes, 5+ MB each). Confining to the active scene drops
        # a single-species export to 25 nodes / ~40 KB.
        use_active_scene=True,
        # `export_apply=True` was the default instinct (bake modifier
        # output into the exported mesh) but in Blender 5.1 it leaks
        # the per-evaluation mesh copies back into `bpy.data`, and on
        # a loop over 178 species each subsequent export picks up
        # every prior species' mesh data. First species = 2.4 MB,
        # last = 10.9 MB — monotonic growth until the whole library
        # is embedded in each GLB. The library has no modifiers, so
        # skip modifier eval entirely and zero the species parent's
        # transform pre-export (below) to pin every GLB at origin.
        export_apply=False,
        # Animations don't exist on this pack; explicit-off saves time.
        export_animations=False,
        # Unlit PBR keeps the authored flat-shaded look consistent with
        # Meshtint / Quaternius exports elsewhere in the pipeline.
        export_materials="EXPORT",
        # Cache-friendly: let the gltf exporter skip image re-encoding
        # when it can.
        export_image_format="AUTO",
    )
    empty.location = saved_loc
    empty.rotation_euler = saved_rot


def enumerate_species(blend_path: Path) -> list[tuple[str, str, str]]:
    """Open the .blend once, walk the tree, return
    `(category, species_name, species_type)` tuples. Closed before
    the per-species export loop so each export pass starts from a
    pristine file."""
    bpy.ops.wm.open_mainfile(filepath=str(blend_path))
    root = bpy.data.objects.get("ANIMALS")
    if root is None:
        sys.exit("no ANIMALS root empty in the .blend — layout changed?")
    out: list[tuple[str, str, str]] = []
    for category in sorted(root.children, key=lambda o: o.name):
        for species in sorted(category.children, key=lambda o: o.name):
            out.append((category.name, species.name, species.type))
    return out


def export_one(blend_path: Path, category: str, name: str, obj_type: str) -> bool:
    """Reopen the .blend fresh and export a single species. Reopening
    is the only reliable way to reset Blender's scene state — in 5.1
    something in `bpy.data` accumulates exported mesh copies across
    iterations and each subsequent `export_scene.gltf` call picks up
    every prior species' data (empirical: file 1 ≈ 40 KB, file 178 ≈
    11 MB). Per-species reopen is slow (~1.5 s/species) but
    correct."""
    bpy.ops.wm.open_mainfile(filepath=str(blend_path))

    # The library's stage `Plane` has a particle system that scatters
    # copies of the species meshes across the scene for the author's
    # preview renders. The gltf exporter walks the evaluated depsgraph
    # and emits a scene-root node for every particle instance —
    # a single-species export ends up with a dozen extra scattered
    # copies of the mesh at weird transforms. Delete the Plane
    # (destroying its particle system) before touching the selection
    # so the depsgraph has nothing but the actual species objects.
    plane = bpy.data.objects.get("Plane")
    if plane is not None:
        bpy.data.objects.remove(plane, do_unlink=True)

    species = bpy.data.objects.get(name)
    if species is None:
        return False
    if obj_type == "EMPTY":
        return export_species(name, species, category)
    if obj_type == "MESH":
        bpy.ops.object.select_all(action="DESELECT")
        species.select_set(True)
        bpy.context.view_layer.objects.active = species
        saved_loc = tuple(species.location)
        saved_rot = tuple(species.rotation_euler)
        species.location = (0.0, 0.0, 0.0)
        species.rotation_euler = (0.0, 0.0, 0.0)
        out = OUT_DIR / f"{name}.glb"
        bpy.ops.export_scene.gltf(
            filepath=str(out),
            export_format="GLB",
            use_selection=True,
            use_active_scene=True,
            export_apply=False,
            export_animations=False,
            export_materials="EXPORT",
            export_image_format="AUTO",
        )
        species.location = saved_loc
        species.rotation_euler = saved_rot
        return True
    return False


def main() -> None:
    blend = ensure_blend()
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    species_list = enumerate_species(blend)
    print(f"found {len(species_list)} species to export")

    exported = 0
    skipped = []
    for category, name, obj_type in species_list:
        if export_one(blend, category, name, obj_type):
            exported += 1
            print(f"  [{category:16s}] {name:30s} → {name}.glb")
        else:
            skipped.append(f"{category}/{name} ({obj_type})")

    print(f"\n=== extracted {exported} species GLBs to {OUT_DIR} ===")
    if skipped:
        print(f"skipped {len(skipped)} nodes:")
        for s in skipped:
            print(f"  - {s}")


if __name__ == "__main__":
    main()
