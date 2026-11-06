"""Creation packaging boundary: exact bytes, fixed closure, identity and atomic failure."""

import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import struct
import zlib
import tarfile
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import package_app
import package_creation


class CreationDistributionTests(unittest.TestCase):
    @unittest.skipUnless(sys.platform == "linux" and shutil.which("readelf"), "Linux ELF tooling required")
    def test_reusable_kit_and_creation_boundary(self):
        """One real-ELF fixture crosses kit preparation, relocation and bounded publication."""
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            root = base / "checkout"
            root.mkdir()
            for name in ("LICENSE-MIT", "LICENSE-APACHE", "THIRD_PARTY_NOTICES.txt"):
                package_app.put(root, name, (name + " fixture\n").encode())
            large_notice = b"UPSTREAM LICENSE TEXT\n" + b"*" * (package_creation.MIB + 1)
            package_app.put(root, "THIRD_PARTY_NOTICES.txt", large_notice)
            package_app.put(root, "packaging/license-sources.json", b'{"texts":[]}\n')
            for name in package_creation.HELPER_FILES:
                package_app.put(root, name, (package_creation.ROOT / name).read_bytes())
            package_app.put(root, ".env", b"PRIVATE CHECKOUT SECRET")
            binary = Path(shutil.which("true")).resolve()
            app = {"id": "motion", "name": "ruster-motion-lab", "version": "0.1.0", "source": None}
            metadata = {"packages": [app], "resolve": {"nodes": [{"id": "motion", "deps": []}]}}
            kit_archive = base / "runtime.tar.gz"
            with patch.object(package_app, "metadata", return_value=metadata) as cargo_metadata, \
                    patch.object(package_app, "notices", return_value=(large_notice, [])), \
                    patch.object(package_creation, "run_runtime", return_value=package_creation.PROFILE) as profile:
                prepared = package_creation.prepare_runtime(root, binary, kit_archive)
            cargo_metadata.assert_called_once_with(root, "ruster-motion-lab")
            profile.assert_called_once_with(binary, ["--creation-info"])
            with tarfile.open(kit_archive) as archive:
                archive.extractall(base / "relocated", filter="data")
            kit = base / "relocated/ruster-runtime"
            runtime_json = (kit / "runtime.json").read_bytes()
            self.assertEqual(prepared["runtime_kit_sha256"], hashlib.sha256(runtime_json).hexdigest())
            self.assertEqual(package_creation.inventory(kit), package_creation.KIT_FILES | {"runtime.json"})
            # The three copied helpers must import without a source checkout/new_app module.
            help_result = subprocess.run([sys.executable, str(kit / "scripts/package_creation.py"), "--help"],
                                         cwd=base, capture_output=True, text=True, check=True)
            self.assertIn("prepare-runtime", help_result.stdout)
            self.assertFalse((kit / "scripts/__pycache__").exists())
            content_root = base / "content"
            content_root.mkdir()
            scene = {"format": "ruster-scene", "format_version": 5,
                     "scene": {"id": "owned-scene", "revision": 4, "objects": [
                         {"id": "marker", "transform": {"translation": [0.1, 0, 0],
                          "rotation": [0, 0, 0], "scale": [1, 1, 1]},
                          "color": [0.12, 0.58, 0.85, 1], "mesh": "triangle"}]},
                     "authoring": {"sources": {}, "bindings": {}}, "undo": [], "redo": []}
            # Deliberate whitespace: saved bytes must survive without JSON re-encoding.
            scene_bytes = json.dumps(scene, indent=3).encode() + b"\n\n"
            mesh = b"v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n"
            content = {"schema": 1, "title": "Creator's 世界", "tick_millis": 100,
                       "scene_sha256": hashlib.sha256(scene_bytes).hexdigest(),
                       "meshes": {"triangle": {"path": "meshes/0000.obj", "sha256": hashlib.sha256(mesh).hexdigest()}}}
            package_app.put(content_root, "scene.json", scene_bytes)
            package_app.put(content_root, "content.json", package_creation.encoded(content))
            package_app.put(content_root, "meshes/0000.obj", mesh)
            package_app.put(content_root, "private.obj", b"PRIVATE PROJECT SECRET")
            response = {"mode": "creation", "title": content["title"], "tick": "0", "fixed_dt": 0.1,
                        "authored_scene": package_creation.typed_scene(scene)}
            output = base / "creation.tar.gz"
            # Packaging itself must use only the prepared kit, never Cargo/readelf/notices.
            with patch.object(package_app, "metadata", side_effect=AssertionError("Cargo during export")), \
                    patch.object(package_app, "elf_requirements", side_effect=AssertionError("readelf during export")), \
                    patch.object(package_app, "notices", side_effect=AssertionError("notices during export")), \
                    patch.object(package_creation, "run_runtime", return_value=response):
                receipt = package_creation.package(kit, content_root, output)
                original = output.read_bytes()
                with tarfile.open(output) as archive:
                    members = archive.getmembers()
                    self.assertTrue(all(m.isfile() and m.name.startswith("application/")
                                        and ".." not in Path(m.name).parts for m in members))
                    archive.extractall(base / "application-relocated", filter="data")
                application = base / "application-relocated/application"
                self.assertNotIn("initial_camera", json.loads((application / "app.json").read_bytes()))
                self.assertEqual((application / "resources/scene.json").read_bytes(), scene_bytes)
                self.assertEqual((application / "resources/meshes/0000.obj").read_bytes(), mesh)
                for name in ("THIRD_PARTY_NOTICES.txt", "SUPPLEMENTAL_NOTICES.txt"):
                    self.assertEqual((application / name).read_bytes(), large_notice)
                self.assertFalse((application / "scripts").exists())
                self.assertFalse((application / "content.json").exists())
                manifest_bytes = (application / "manifest.json").read_bytes()
                manifest = json.loads(manifest_bytes)
                self.assertEqual(receipt["manifest_sha256"], hashlib.sha256(manifest_bytes).hexdigest())
                self.assertEqual(receipt["archive_sha256"], hashlib.sha256(original).hexdigest())
                for record in manifest["files"]:
                    data = (application / record["path"]).read_bytes()
                    self.assertEqual(hashlib.sha256(data).hexdigest(), record["sha256"])
                    self.assertNotIn(b"PRIVATE", data)
                with self.assertRaisesRegex(ValueError, "overwrite"):
                    package_creation.package(kit, content_root, output)
                self.assertEqual(output.read_bytes(), original)
                failed = base / "failed.tar.gz"
                def rejected():
                    with self.assertRaises((ValueError, OSError)):
                        package_creation.package(kit, content_root, failed)
                    self.assertFalse(failed.exists())
                # Explicit framing is frozen launch intent; legacy kits cannot
                # silently drop it, and the capable runtime must echo it at preflight.
                camera = {"target": [0.1, 2.3, -0.7], "yaw": 0.4, "pitch": 0.3,
                          "distance": 7.1, "orthographic": True}
                normalized_camera = package_creation.initial_camera(camera)
                self.assertNotEqual(normalized_camera, camera)
                framed = {**content, "initial_camera": camera}
                (content_root / "content.json").write_bytes(package_creation.encoded(framed))
                with self.assertRaisesRegex(ValueError, "does not support initial_camera"):
                    package_creation.package(kit, content_root, failed)
                self.assertFalse(failed.exists())
                capable = json.loads(runtime_json)
                capable["profile"] = package_creation.CAMERA_PROFILE
                (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                # The actual Rust headless path serializes through Value, which
                # promotes f32 to its exact f64 value before JSON encoding.
                runtime_camera = {"target": [0.10000000149011612, 2.299999952316284, -0.699999988079071],
                                  "yaw": 0.4000000059604645, "pitch": 0.30000001192092896,
                                  "distance": 7.099999904632568, "orthographic": True}
                self.assertEqual(runtime_camera, normalized_camera)
                framed_response = {**response, "initial_camera": runtime_camera}
                framed_output = base / "framed.tar.gz"
                with patch.object(package_creation, "run_runtime", return_value=framed_response):
                    framed_receipt = package_creation.package(kit, content_root, framed_output)
                self.assertEqual(framed_receipt["initial_camera"], normalized_camera)
                with tarfile.open(framed_output) as archive:
                    descriptor = json.load(archive.extractfile("application/app.json"))
                    self.assertEqual(descriptor["initial_camera"], normalized_camera)
                    self.assertEqual(archive.extractfile("application/resources/scene.json").read(), scene_bytes)
                with self.assertRaisesRegex(ValueError, "explicit initial camera"):
                    package_creation.package(kit, content_root, failed)
                self.assertFalse(failed.exists())
                for field, value in [("distance", 0), ("pitch", 2), ("yaw", 1e100),
                                     ("orthographic", 1), ("target", [True, 0, 0])]:
                    invalid_camera = {**camera, field: value}
                    (content_root / "content.json").write_bytes(package_creation.encoded(
                        {**content, "initial_camera": invalid_camera}))
                    rejected()
                (kit / "runtime.json").write_bytes(runtime_json)
                (content_root / "content.json").write_bytes(package_creation.encoded(content))
                # Scene6 uses the exact v2 capability profile and an independently
                # derived closure; unused material textures are deliberately absent.
                textured_scene = copy.deepcopy(scene)
                textured_scene["format_version"] = 6
                textured_scene["scene"]["objects"][0]["material"] = "paint"
                textured_scene["scene"]["materials"] = {
                    "paint": {"name": None, "tint": [0.1, 0.5, 1], "base_color_texture": "albedo",
                              "filter": "linear", "address": "repeat"},
                    "unused": {"tint": [1, 1, 1], "base_color_texture": "not-exported",
                               "filter": "nearest", "address": "clamp"}}
                textured_bytes = json.dumps(textured_scene, indent=3).encode() + b"\n\n"
                def png_chunk(kind, data):
                    return (struct.pack(">I", len(data)) + kind + data
                            + struct.pack(">I", zlib.crc32(kind + data)))
                png = (b"\x89PNG\r\n\x1a\n"
                       + png_chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 6, 0, 0, 0))
                       + png_chunk(b"IDAT", zlib.compress(b"\0\x25\x80\xff\0"))
                       + png_chunk(b"IEND", b""))
                textured = {**content, "schema": 2, "scene_sha256": package_creation.sha256(textured_bytes),
                            "initial_camera": camera,
                            "textures": {"albedo": {"path": "textures/0000.png",
                                                    "sha256": package_creation.sha256(png)}}}
                package_app.put(content_root, "textures/0000.png", png)
                (content_root / "scene.json").write_bytes(textured_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(textured))
                for legacy_profile in (package_creation.PROFILE, package_creation.CAMERA_PROFILE):
                    capable["profile"] = legacy_profile
                    (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                    with self.assertRaisesRegex(ValueError, "Scene6 opaque_materials"):
                        package_creation.package(kit, content_root, failed)
                    self.assertFalse(failed.exists())
                capable["profile"] = package_creation.MATERIAL_PROFILE
                (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                textured_response = {**framed_response, "authored_scene": package_creation.typed_scene(textured_scene)}
                self.assertEqual(textured_response["authored_scene"]["materials"]["paint"]["tint"],
                                 [0.10000000149011612, 0.5, 1.0])
                self.assertNotIn("name", textured_response["authored_scene"]["materials"]["paint"])
                material_output = base / "material.tar.gz"
                with patch.object(package_creation, "run_runtime", return_value=textured_response):
                    material_receipt = package_creation.package(kit, content_root, material_output)
                self.assertEqual(set(material_receipt), set(framed_receipt))
                with tarfile.open(material_output) as archive:
                    descriptor = json.load(archive.extractfile("application/app.json"))
                    self.assertEqual(descriptor["version"], 2)
                    self.assertEqual(descriptor["textures"], {"albedo": "resources/textures/0000.png"})
                    self.assertEqual(descriptor["initial_camera"], normalized_camera)
                    self.assertEqual(archive.extractfile("application/resources/scene.json").read(), textured_bytes)
                    self.assertEqual(archive.extractfile("application/resources/textures/0000.png").read(), png)
                    material_manifest = json.load(archive.extractfile("application/manifest.json"))
                    self.assertEqual(material_manifest["profile"], package_creation.MATERIAL_PROFILE)
                    self.assertEqual([record for record in material_manifest["files"]
                                      if record["path"] == "resources/textures/0000.png"],
                                     [{"path": "resources/textures/0000.png", "bytes": len(png),
                                       "sha256": package_creation.sha256(png)}])
                # Scene7 has a new exact capability/schema but keeps descriptor2.
                # Lighting is scene state, never an extra resource dependency.
                lit_scene = copy.deepcopy(textured_scene)
                lit_scene["format_version"] = 7
                lit_scene["scene"]["lighting"] = {
                    "mode": "authored", "ambient": {"color": [0.1, 0.5, 1], "intensity": 0.28},
                    "lights": {"point": {"name": None,
                        "transform": {"translation": [0.1, 2, 0], "rotation": [0, 0.4, 0], "scale": [1, 1, 1]},
                        "enabled": True, "color": [0.2, 0.5, 1], "intensity": 0.72,
                        "kind": {"type": "point", "range": 0.1}}}}
                lit_bytes = package_creation.encoded(lit_scene)
                lit_content = {**textured, "schema": 3, "scene_sha256": package_creation.sha256(lit_bytes)}
                (content_root / "scene.json").write_bytes(lit_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(lit_content))
                for old_profile in (package_creation.PROFILE, package_creation.CAMERA_PROFILE,
                                    package_creation.MATERIAL_PROFILE):
                    capable["profile"] = old_profile
                    (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                    with patch.object(package_creation, "run_runtime", side_effect=AssertionError("old-kit preflight")):
                        with self.assertRaisesRegex(ValueError, "Scene7 authored_lighting"):
                            package_creation.package(kit, content_root, failed)
                    self.assertFalse(failed.exists())
                capable["profile"] = package_creation.LIGHTING_PROFILE
                (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                lit_response = {**framed_response, "authored_scene": package_creation.typed_scene(lit_scene)}
                typed_lighting = lit_response["authored_scene"]["lighting"]
                self.assertEqual(typed_lighting["ambient"]["intensity"], 0.2800000011920929)
                self.assertEqual(typed_lighting["lights"]["point"]["kind"]["range"], 0.10000000149011612)
                self.assertNotIn("name", typed_lighting["lights"]["point"])
                with patch.object(package_creation, "run_runtime", return_value=lit_response):
                    lit_output = base / "lighting.tar.gz"
                    package_creation.package(kit, content_root, lit_output)
                with tarfile.open(lit_output) as archive:
                    descriptor = json.load(archive.extractfile("application/app.json"))
                    self.assertEqual(descriptor["version"], 2)
                    self.assertEqual(set(descriptor), {"format", "version", "title", "scene", "tick_millis",
                                                       "meshes", "textures", "initial_camera"})
                    self.assertEqual(archive.extractfile("application/resources/scene.json").read(), lit_bytes)
                    self.assertEqual(json.load(archive.extractfile("application/manifest.json"))["profile"],
                                     package_creation.LIGHTING_PROFILE)
                wrong_lighting = copy.deepcopy(lit_response)
                wrong_lighting["authored_scene"]["lighting"]["lights"]["point"]["intensity"] = 1
                with patch.object(package_creation, "run_runtime", return_value=wrong_lighting):
                    rejected()
                for schema in (1, 2):
                    (content_root / "content.json").write_bytes(package_creation.encoded({**lit_content, "schema": schema}))
                    rejected()
                legacy_lighting = copy.deepcopy(lit_scene)
                legacy_lighting["scene"]["lighting"] = {"mode": "legacy"}
                self.assertNotIn("lighting", package_creation.typed_scene(legacy_lighting))
                # Scene8 always selects a new exact profile, even with no shadow
                # field/enabled Sun. Light resources remain scene data only.
                shadow_scene = copy.deepcopy(lit_scene)
                shadow_scene["format_version"] = 8
                shadow_bytes = package_creation.encoded(shadow_scene)
                shadow_content = {**lit_content, "schema": 4,
                                  "scene_sha256": package_creation.sha256(shadow_bytes)}
                (content_root / "scene.json").write_bytes(shadow_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(shadow_content))
                for old_profile in (package_creation.PROFILE, package_creation.CAMERA_PROFILE,
                                    package_creation.MATERIAL_PROFILE, package_creation.LIGHTING_PROFILE):
                    capable["profile"] = old_profile
                    (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                    with patch.object(package_creation, "run_runtime", side_effect=AssertionError("old-kit preflight")):
                        with self.assertRaisesRegex(ValueError, "Scene8 sun_shadows"):
                            package_creation.package(kit, content_root, failed)
                    self.assertFalse(failed.exists())
                sun = copy.deepcopy(shadow_scene["scene"]["lighting"]["lights"]["point"])
                sun["kind"] = {"type": "directional", "shadow": {"enabled": True, "quality": "high",
                    "depth_bias_texels": 0.15, "normal_bias_texels": 0.1}}
                shadow_scene["scene"]["lighting"]["lights"]["sun"] = sun
                shadow_bytes = package_creation.encoded(shadow_scene)
                shadow_content["scene_sha256"] = package_creation.sha256(shadow_bytes)
                (content_root / "scene.json").write_bytes(shadow_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(shadow_content))
                capable["profile"] = package_creation.SUN_SHADOW_PROFILE
                (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                shadow_response = {**framed_response, "authored_scene": package_creation.typed_scene(shadow_scene)}
                typed_shadow = shadow_response["authored_scene"]["lighting"]["lights"]["sun"]["kind"]["shadow"]
                self.assertEqual(typed_shadow["depth_bias_texels"], 0.15000000596046448)
                self.assertEqual(typed_shadow["normal_bias_texels"], 0.10000000149011612)
                shadow_output = base / "sun-shadow.tar.gz"
                with patch.object(package_creation, "run_runtime", return_value=shadow_response):
                    package_creation.package(kit, content_root, shadow_output)
                with tarfile.open(shadow_output) as archive:
                    descriptor = json.load(archive.extractfile("application/app.json"))
                    self.assertEqual(descriptor["version"], 2)
                    self.assertEqual(set(descriptor), {"format", "version", "title", "scene", "tick_millis",
                                                       "meshes", "textures", "initial_camera"})
                    self.assertEqual(archive.extractfile("application/resources/scene.json").read(), shadow_bytes)
                    self.assertEqual(json.load(archive.extractfile("application/manifest.json"))["profile"],
                                     package_creation.SUN_SHADOW_PROFILE)
                # Scene9 requires the exact Surface profile and descriptor3 normals,
                # even when all new scene settings are omitted/default.
                surface_scene = copy.deepcopy(shadow_scene)
                surface_scene["format_version"] = 9
                surface_bytes = package_creation.encoded(surface_scene)
                surface_content = {**shadow_content, "schema":5,
                    "scene_sha256":package_creation.sha256(surface_bytes),
                    "meshes":{identity:{**record,"normal_policy":"authored_when_complete"}
                              for identity,record in shadow_content["meshes"].items()}}
                (content_root / "scene.json").write_bytes(surface_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(surface_content))
                with patch.object(package_creation, "run_runtime", side_effect=AssertionError("old-kit preflight")):
                    with self.assertRaisesRegex(ValueError, "Scene9 surface_materials"):
                        package_creation.package(kit, content_root, failed)
                self.assertFalse(failed.exists())
                capable["profile"] = package_creation.SURFACE_PROFILE
                (kit / "runtime.json").write_bytes(package_creation.encoded(capable))
                surface_scene["scene"]["display"]={"type":"surface_hdr","exposure_stops":1.0}
                surface_scene["scene"]["materials"]["paint"]["response"]={"type":"dielectric","roughness":.08}
                surface_bytes=package_creation.encoded(surface_scene)
                surface_content["scene_sha256"]=package_creation.sha256(surface_bytes)
                (content_root / "scene.json").write_bytes(surface_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(surface_content))
                surface_response={**framed_response,"authored_scene":package_creation.typed_scene(surface_scene)}
                with patch.object(package_creation,"run_runtime",return_value=surface_response):
                    surface_output=base/"surface.tar.gz"
                    package_creation.package(kit,content_root,surface_output)
                with tarfile.open(surface_output) as archive:
                    descriptor=json.load(archive.extractfile("application/app.json"))
                    self.assertEqual(descriptor["version"],3)
                    self.assertEqual(descriptor["mesh_normal_policies"],
                        {identity:"authored_when_complete" for identity in surface_content["meshes"]})
                    self.assertEqual(json.load(archive.extractfile("application/manifest.json"))["profile"],package_creation.SURFACE_PROFILE)
                for bad_policy in (None,True,"smooth",{}):
                    bad_content=copy.deepcopy(surface_content)
                    next(iter(bad_content["meshes"].values()))["normal_policy"]=bad_policy
                    (content_root/"content.json").write_bytes(package_creation.encoded(bad_content))
                    with self.assertRaisesRegex(ValueError,"normal policy"):
                        package_creation.source_content(content_root,None)
                (content_root/"content.json").write_bytes(package_creation.encoded(surface_content))
                # Scene10 requires descriptor4/roles even without an assigned
                # map. The first old-kit rejection must precede runtime calls.
                mapped_scene = copy.deepcopy(surface_scene)
                mapped_scene["format_version"] = 10
                no_map_bytes = package_creation.encoded(mapped_scene)
                mapped_content = {**surface_content, "schema":6,
                    "scene_sha256":package_creation.sha256(no_map_bytes),
                    "textures":{identity:{**record,"texture_role":"base_color_srgb"}
                                for identity,record in surface_content["textures"].items()}}
                (content_root/"scene.json").write_bytes(no_map_bytes)
                (content_root/"content.json").write_bytes(package_creation.encoded(mapped_content))
                with patch.object(package_creation,"run_runtime",side_effect=AssertionError("old-kit preflight")):
                    with self.assertRaisesRegex(ValueError,"Scene10 roughness_maps"):
                        package_creation.package(kit,content_root,failed)
                self.assertFalse(failed.exists())
                mapped_scene["scene"]["materials"]["paint"]["roughness_texture"] = "finish"
                mapped_scene["scene"]["materials"]["paint"].pop("response")
                mapped_scene["scene"]["materials"]["unused"]["roughness_texture"] = "unused-map"
                mapped_scene["undo"] = [{"scene":[],"materials":[{"id":"history-only",
                    "before":{"tint":[1,1,1],"filter":"linear","address":"repeat","roughness_texture":"history-map"},
                    "after":None}]}]
                mapped_bytes = package_creation.encoded(mapped_scene)
                mapped_content["scene_sha256"] = package_creation.sha256(mapped_bytes)
                mapped_content["textures"]["finish"] = {"path":"textures/0001.png",
                    "sha256":package_creation.sha256(png),"texture_role":"roughness_linear"}
                package_app.put(content_root,"textures/0001.png",png)
                (content_root/"scene.json").write_bytes(mapped_bytes)
                (content_root/"content.json").write_bytes(package_creation.encoded(mapped_content))
                capable["profile"] = package_creation.ROUGHNESS_PROFILE
                (kit/"runtime.json").write_bytes(package_creation.encoded(capable))
                self.assertEqual(package_creation.scene_dependencies(mapped_scene),({"triangle"},{"albedo","finish"}))
                mapped_response={**framed_response,"authored_scene":package_creation.typed_scene(mapped_scene)}
                mapped_output=base/"roughness.tar.gz"
                with patch.object(package_creation,"run_runtime",return_value=mapped_response):
                    package_creation.package(kit,content_root,mapped_output)
                with tarfile.open(mapped_output) as archive:
                    descriptor=json.load(archive.extractfile("application/app.json"))
                    self.assertEqual(descriptor["version"],4)
                    self.assertEqual(descriptor["texture_roles"],{"albedo":"base_color_srgb","finish":"roughness_linear"})
                    self.assertEqual(archive.extractfile("application/resources/scene.json").read(),mapped_bytes)
                for mutation in ("missing-role","null-role","wrong-role","unknown-role","extra-record-field","old-schema"):
                    bad=copy.deepcopy(mapped_content)
                    if mutation=="missing-role": bad["textures"]["finish"].pop("texture_role")
                    elif mutation=="null-role": bad["textures"]["finish"]["texture_role"]=None
                    elif mutation=="wrong-role": bad["textures"]["finish"]["texture_role"]="base_color_srgb"
                    elif mutation=="unknown-role": bad["textures"]["finish"]["texture_role"]="normal"
                    elif mutation=="extra-record-field": bad["textures"]["finish"]["channel"]="red"
                    else: bad["schema"]=5
                    (content_root/"content.json").write_bytes(package_creation.encoded(bad))
                    with self.subTest(roughness_record=mutation),self.assertRaises(ValueError):
                        package_creation.source_content(content_root,None)
                # The new kit accepts the unchanged old descriptor3 route.
                (content_root/"scene.json").write_bytes(surface_bytes)
                (content_root/"content.json").write_bytes(package_creation.encoded(surface_content))
                with patch.object(package_creation,"run_runtime",return_value=surface_response):
                    package_creation.package(kit,content_root,base/"surface-with-roughness-kit.tar.gz")
                # Current kit retains the Scene7/schema3 route as well.
                (content_root / "scene.json").write_bytes(lit_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(lit_content))
                with patch.object(package_creation, "run_runtime", return_value=lit_response):
                    package_creation.package(kit, content_root, base / "lighting-with-shadow-kit.tar.gz")
                # The new kit still packages Scene6; the old Scene5 fixture below
                # exercises the older descriptor1 path through this same kit.
                (content_root / "scene.json").write_bytes(textured_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(textured))
                with patch.object(package_creation, "run_runtime", return_value=textured_response):
                    package_creation.package(kit, content_root, base / "material-with-lighting-kit.tar.gz")
                # A claimed texture cannot hide missing dependencies or smuggle
                # unrelated source bytes, kinds, paths or a second identity.
                for mutation in ("missing", "extra", "wrong-kind", "traversal", "hash", "schema"):
                    invalid = copy.deepcopy(textured)
                    if mutation == "missing":
                        invalid["textures"] = {}
                    elif mutation == "extra":
                        invalid["textures"]["unused"] = invalid["textures"]["albedo"]
                    elif mutation == "wrong-kind":
                        invalid["textures"]["albedo"]["path"] = "meshes/0000.obj"
                    elif mutation == "traversal":
                        invalid["textures"]["albedo"]["path"] = "textures/../private.obj"
                    elif mutation == "hash":
                        invalid["textures"]["albedo"]["sha256"] = "0" * 64
                    else:
                        invalid["schema"] = 1
                    (content_root / "content.json").write_bytes(package_creation.encoded(invalid))
                    with self.subTest(texture_mutation=mutation):
                        rejected()
                (content_root / "content.json").write_bytes(package_creation.encoded(textured))
                texture_path = content_root / "textures/0000.png"
                texture_path.unlink()
                texture_path.symlink_to(content_root / "private.obj")
                rejected()
                texture_path.unlink()
                with texture_path.open("wb") as oversized:
                    oversized.truncate(package_creation.MAX_TEXTURE_BYTES + 1)
                rejected()
                texture_path.write_bytes(png)
                with patch.object(package_creation, "run_runtime", side_effect=ValueError("invalid PNG or missing UVs")):
                    rejected()
                wrong_tint = copy.deepcopy(textured_response)
                wrong_tint["authored_scene"]["materials"]["paint"]["tint"][0] = 0.1
                with patch.object(package_creation, "run_runtime", return_value=wrong_tint):
                    rejected()
                for invalid_profile in (
                        {**package_creation.MATERIAL_PROFILE, "descriptor_version": 1},
                        {**package_creation.MATERIAL_PROFILE, "capabilities": ["opaque_materials"]},
                        {**package_creation.MATERIAL_PROFILE, "capabilities": ["opaque_materials", "initial_camera"]},
                        {**package_creation.LIGHTING_PROFILE, "capabilities": ["initial_camera", "opaque_materials", "authored_lighting", "shadows"]},
                        {**package_creation.LIGHTING_PROFILE, "capabilities": ["authored_lighting", "initial_camera", "opaque_materials"]},
                        {**package_creation.SUN_SHADOW_PROFILE, "capabilities": ["initial_camera", "opaque_materials", "sun_shadows", "authored_lighting"]},
                        {**package_creation.SUN_SHADOW_PROFILE, "capabilities": ["initial_camera", "opaque_materials", "authored_lighting", "sun_shadows", "point_shadows"]},
                        {**package_creation.ROUGHNESS_PROFILE, "descriptor_version":3},
                        {**package_creation.ROUGHNESS_PROFILE, "capabilities": package_creation.ROUGHNESS_PROFILE["capabilities"][:-2]+["roughness_maps","surface_materials"]},
                        {**package_creation.MATERIAL_PROFILE, "version": True}):
                    with self.assertRaisesRegex(ValueError, "unsupported runtime creation profile"):
                        package_creation.check_profile(invalid_profile)
                # Current runtimes preserve the old descriptor1 path as well.
                (content_root / "scene.json").write_bytes(scene_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(content))
                with patch.object(package_creation, "run_runtime", return_value=response):
                    legacy_output = base / "legacy-with-current-kit.tar.gz"
                    package_creation.package(kit, content_root, legacy_output)
                with tarfile.open(legacy_output) as archive:
                    legacy_descriptor = json.load(archive.extractfile("application/app.json"))
                    self.assertEqual(legacy_descriptor["version"], 1)
                    self.assertNotIn("textures", legacy_descriptor)
                overlapping = copy.deepcopy(textured_scene)
                overlapping["scene"]["materials"]["paint"]["base_color_texture"] = "triangle"
                overlapping_bytes = package_creation.encoded(overlapping)
                (content_root / "scene.json").write_bytes(overlapping_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(
                    {**textured, "scene_sha256": package_creation.sha256(overlapping_bytes),
                     "textures": {"triangle": textured["textures"]["albedo"]}}))
                with self.assertRaisesRegex(ValueError, "dependency closure"):
                    package_creation.package(kit, content_root, failed)
                self.assertFalse(failed.exists())
                # Scene6 still requires v2 for a material with no texture. There
                # is no inference that an empty texture map makes old runtimes safe.
                no_texture = copy.deepcopy(textured_scene)
                no_texture["scene"]["materials"]["paint"].pop("base_color_texture")
                no_texture_bytes = package_creation.encoded(no_texture)
                (content_root / "scene.json").write_bytes(no_texture_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(
                    {**textured, "textures": {}, "scene_sha256": package_creation.sha256(no_texture_bytes)}))
                (kit / "runtime.json").write_bytes(runtime_json)
                with self.assertRaisesRegex(ValueError, "Scene6 opaque_materials"):
                    package_creation.package(kit, content_root, failed)
                self.assertFalse(failed.exists())
                (content_root / "scene.json").write_bytes(scene_bytes)
                (content_root / "content.json").write_bytes(package_creation.encoded(content))
                # Exact bounded kit inventory and hashed bytes, including nested duplicate JSON keys.
                for mutation in ("tamper", "extra", "duplicate-record", "traversal", "oversize", "duplicate-key"):
                    with self.subTest(mutation=mutation):
                        value = json.loads(runtime_json)
                        if mutation == "tamper":
                            (kit / "README.txt").write_bytes(b"modified")
                        elif mutation == "extra":
                            (kit / "private.txt").write_bytes(b"private")
                        elif mutation == "duplicate-record":
                            value["files"][-1] = value["files"][0]
                        elif mutation == "traversal":
                            value["files"][0]["path"] = "../escape"
                        elif mutation == "oversize":
                            value["files"][0]["bytes"] = package_creation.MAX_BINARY_BYTES + 1
                        else:
                            (kit / "runtime.json").write_bytes(runtime_json.replace(b'"version": 1', b'"version":1,"version":1', 1))
                        if mutation not in ("tamper", "extra", "duplicate-key"):
                            (kit / "runtime.json").write_bytes(package_creation.encoded(value))
                        rejected()
                        (kit / "runtime.json").write_bytes(runtime_json)
                        (kit / "private.txt").unlink(missing_ok=True)
                        with tarfile.open(kit_archive) as archive:
                            readme = archive.extractfile("ruster-runtime/README.txt").read()
                        (kit / "README.txt").write_bytes(readme)
                # Changed/missing/symlinked references never cause recursive fallback or publication.
                (content_root / "scene.json").write_bytes(scene_bytes + b" ")
                rejected()
                (content_root / "scene.json").write_bytes(scene_bytes)
                (content_root / "meshes/0000.obj").unlink()
                rejected()
                (content_root / "meshes/0000.obj").symlink_to(content_root / "private.obj")
                rejected()
                (content_root / "meshes/0000.obj").unlink()
                (content_root / "meshes/0000.obj").write_bytes(mesh)
                with patch.object(package_creation, "run_runtime", side_effect=ValueError("invalid Lua")):
                    rejected()
                wrong = copy.deepcopy(response)
                wrong["authored_scene"]["objects"][0]["color"][0] = 0.12
                self.assertNotEqual(wrong["authored_scene"], response["authored_scene"])
                with patch.object(package_creation, "run_runtime", return_value=wrong):
                    rejected()
                cancellation = base / "cancel"
                cancellation.touch()
                with self.assertRaisesRegex(ValueError, "cancelled"):
                    package_creation.package(kit, content_root, failed, cancellation)
                self.assertFalse(failed.exists())

    def test_sun_shadow_admission_uses_only_owning_current_and_history_fields(self):
        """Strict settings cannot hide in history or be confused with behavior data."""
        shadow = {"enabled": False, "quality": "standard", "depth_bias_texels": 0.15,
                  "normal_bias_texels": 0}
        sun = {"transform": {"translation": [0, 2, 0], "rotation": [0, 0, 0], "scale": [1, 1, 1]},
               "enabled": True, "color": [1, 1, 1], "intensity": 0.72,
               "kind": {"type": "directional", "shadow": shadow}}
        lighting = {"mode": "authored", "ambient": {"color": [1, 1, 1], "intensity": 0.28},
                    "lights": {"sun": sun}}
        document = {"format": "ruster-scene", "format_version": 8,
                    "scene": {"id": "shadow-admission", "revision": 0, "objects": [], "lighting": lighting},
                    "authoring": {"bindings": {"behavior": {"properties": {"shadow": {"type": "bool", "value": True}}}}},
                    "undo": [], "redo": []}
        self.assertEqual(package_creation.scene_dependencies(document), (set(), set()))
        typed = package_creation.typed_scene(document)
        self.assertEqual(typed["lighting"]["lights"]["sun"]["kind"], {"type": "directional"})
        omitted = copy.deepcopy(document)
        omitted["scene"]["lighting"]["lights"]["sun"]["kind"].pop("shadow")
        self.assertEqual(package_creation.typed_scene(omitted), typed)
        retained = copy.deepcopy(document)
        retained["scene"]["lighting"]["lights"]["sun"]["kind"]["shadow"]["quality"] = "high"
        self.assertIn("shadow", package_creation.typed_scene(retained)["lighting"]["lights"]["sun"]["kind"],
                      "Disabled nondefault settings remain authored data")
        invalid_shadows = [None, {}, {**shadow, "enabled": 0}, {**shadow, "quality": "ultra"},
                           {**shadow, "depth_bias_texels": -1e-50}, {**shadow, "normal_bias_texels": 4.00000001},
                           {**shadow, "depth_bias_texels": True}, {**shadow, "normal_bias_texels": float("inf")},
                           {**shadow, "extra": 0}]
        invalid_shadows.extend({key: value for key, value in shadow.items() if key != missing} for missing in shadow)
        for candidate in invalid_shadows:
            invalid = copy.deepcopy(document)
            invalid["scene"]["lighting"]["lights"]["sun"]["kind"]["shadow"] = candidate
            with self.subTest(shadow=candidate), self.assertRaises(ValueError):
                package_creation.scene_dependencies(invalid)
        for stack, endpoint in (("undo", "before"), ("redo", "after")):
            invalid = copy.deepcopy(omitted)
            delta = {"kind": "lighting", "before": copy.deepcopy(lighting), "after": copy.deepcopy(lighting)}
            invalid[stack] = [{"scene": [copy.deepcopy(delta)]}]
            invalid[stack][0]["scene"][0][endpoint]["lights"]["sun"]["kind"]["shadow"] = None
            with self.subTest(stack=stack, endpoint=endpoint), self.assertRaises(ValueError):
                package_creation.scene_dependencies(invalid)
        point = copy.deepcopy(document)
        point["scene"]["lighting"]["lights"]["sun"]["kind"] = {"type": "point", "range": 5, "shadow": shadow}
        with self.assertRaisesRegex(ValueError, "directional"):
            package_creation.scene_dependencies(point)
        for version in range(1, 8):
            old = copy.deepcopy(document)
            old["format_version"] = version
            with self.subTest(version=version), self.assertRaisesRegex(ValueError, "Scene8 sun_shadows"):
                package_creation.validate_sun_shadows(old)
            old["scene"].pop("lighting")
            # Arbitrary behavior property names are not scene-light definitions.
            package_creation.validate_sun_shadows(old)
            delta = {"kind": "lighting", "before": lighting, "after": {"mode": "legacy"}}
            old["redo"] = [delta] if version <= 4 else [{"scene": [delta]}]
            with self.subTest(history_version=version), self.assertRaisesRegex(ValueError, "Scene8 sun_shadows"):
                package_creation.validate_sun_shadows(old)
        top_level = copy.deepcopy(omitted)
        top_level["scene"]["lighting"]["lights"]["sun"]["shadow"] = shadow
        with self.assertRaisesRegex(ValueError, "kind.shadow"):
            package_creation.scene_dependencies(top_level)

    def test_surface_admission_keeps_original_bounds_and_owning_history(self):
        doc={"format":"ruster-scene","format_version":9,"scene":{"id":"surface","revision":0,"objects":[],
            "display":{"type":"surface_hdr","exposure_stops":0},
            "materials":{"finish":{"tint":[1,1,1],"filter":"linear","address":"repeat",
                "response":{"type":"dielectric","roughness":.08}}}},"undo":[],"redo":[]}
        package_creation.validate_surface_fields(doc)
        self.assertEqual(package_creation.typed_scene(doc)["materials"]["finish"]["response"]["roughness"],.08)
        rounded=copy.deepcopy(doc);rounded["scene"]["materials"]["finish"]["response"]["roughness"]=.0800000001
        self.assertEqual(package_creation.typed_scene(rounded)["materials"]["finish"]["response"]["roughness"],.08)
        for bad in (.079999999999,.07999999821186066,1.0000000001,True,None):
            invalid=copy.deepcopy(doc);invalid["scene"]["materials"]["finish"]["response"]["roughness"]=bad
            with self.subTest(roughness=bad),self.assertRaises(ValueError):package_creation.validate_surface_fields(invalid)
        for bad in (-8.000000001,8.000000001,True,None):
            invalid=copy.deepcopy(doc);invalid["scene"]["display"]["exposure_stops"]=bad
            with self.subTest(exposure=bad),self.assertRaises(ValueError):package_creation.validate_surface_fields(invalid)
        for version in range(1,9):
            invalid=copy.deepcopy(doc);invalid["format_version"]=version
            with self.subTest(version=version),self.assertRaises(ValueError):package_creation.validate_surface_fields(invalid)
        for key in ("display","response"):
            invalid=copy.deepcopy(doc);invalid["scene"].pop("display");invalid["scene"]["materials"].clear();invalid["format_version"]=8
            invalid["undo"]=[{"scene":[{"kind":"display","before":{"type":"legacy_sdr"},"after":{"type":"surface_hdr","exposure_stops":0}}]}] if key=="display" else [{"materials":[{"before":{"response":None},"after":None}]}]
            with self.subTest(history=key),self.assertRaises(ValueError):package_creation.validate_surface_fields(invalid)
        old=copy.deepcopy(doc);old["format_version"]=8;old["scene"].pop("display");old["scene"]["materials"].clear()
        old["authoring"]={"bindings":{"object":{"properties":{"display":1,"response":2}}}}
        package_creation.validate_surface_fields(old)
        mapped=copy.deepcopy(doc);mapped["format_version"]=10
        mapped["scene"]["materials"]["finish"]["roughness_texture"]="finish-map"
        package_creation.validate_surface_fields(mapped)
        self.assertEqual(package_creation.typed_scene(mapped)["materials"]["finish"]["roughness_texture"],"finish-map")
        mapped["scene"]["materials"]["finish"]["roughness_texture"]=None
        self.assertNotIn("roughness_texture",package_creation.typed_scene(mapped)["materials"]["finish"])
        for version in (1,9):
            invalid=copy.deepcopy(mapped);invalid["format_version"]=version
            invalid["scene"].pop("display");invalid["scene"]["materials"]["finish"].pop("response")
            with self.subTest(map_version=version),self.assertRaisesRegex(ValueError,"Scene10"):
                package_creation.validate_surface_fields(invalid)
        for stack,endpoint in (("undo","before"),("redo","after")):
            invalid=copy.deepcopy(old);invalid["format_version"]=9
            invalid[stack]=[{"materials":[{"before":None,"after":None}]}]
            invalid[stack][0]["materials"][0][endpoint]={"roughness_texture":None}
            with self.subTest(map_history=(stack,endpoint)),self.assertRaisesRegex(ValueError,"Scene10"):
                package_creation.validate_surface_fields(invalid)
        old["authoring"]["bindings"]["object"]["properties"]["roughness_texture"]=None
        package_creation.validate_surface_fields(old)

    @unittest.skipUnless(sys.platform == "linux", "Linux process lifecycle required")
    def test_cancellation_reaps_the_owned_preflight_child(self):
        """Cancellation during a running process kills and reaps only that process."""
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            marker, cancel = base / "pid", base / "cancel"
            code = f"import os,time;open({str(marker)!r},'w').write(str(os.getpid()));time.sleep(30)"
            timer = threading.Timer(0.3, cancel.touch)
            timer.start()
            try:
                with self.assertRaisesRegex(ValueError, "cancelled"):
                    package_creation.run_runtime(Path(sys.executable), ["-c", code], cancel)
            finally:
                timer.cancel()
                timer.join()
            self.assertTrue(marker.exists(), "child started before cancellation")
            with self.assertRaises(ProcessLookupError):
                os.kill(int(marker.read_text()), 0)


if __name__ == "__main__":
    unittest.main()
