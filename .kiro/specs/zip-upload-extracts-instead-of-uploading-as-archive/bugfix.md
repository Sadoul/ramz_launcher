# Bugfix Requirements Document

## Introduction

When an administrator uploads a `.zip` file through the launcher's admin panel ("upload mod"), the zip is currently misclassified as a mod, resource pack, or shader pack based on a content heuristic and stored under one of the modpack subfolders (`mods/`, `resourcepacks/`, `shaderpacks/`). The launcher then downloads it on client startup as a single regular file without extracting it, so the inner directory structure inside the zip (`mods/`, `config/`, `shaderpacks/`, etc.) never reaches the corresponding folders inside the modpack's game directory.

The user's intent is the opposite: a zip uploaded via the admin panel should be treated as an opaque archive of the modpack's root layout. The zip should be stored as-is in a dedicated location in the build repository, and at runtime the launcher should extract its contents directly into the modpack's root game directory (`mc_dir`) so the inner `mods/`, `config/`, `shaderpacks/`, etc. folders merge with the existing modpack structure.

This bug affects every administrator workflow where a multi-folder `.zip` is uploaded as a single artifact, and silently ships incomplete modpack contents to all clients of the build.

## Bug Analysis

### Current Behavior (Defect)

When an admin uploads a `.zip` through `upload_build_mod`, the function inspects the zip's contents (looking for `pack.mcmeta` or a `shaders/` prefix) and chooses a destination subfolder among `mods/`, `resourcepacks/`, or `shaderpacks/`. The zip is uploaded under that subfolder as a single file. On the client, `sync_build_files` downloads the zip into the chosen subfolder verbatim and never extracts it.

1.1 WHEN an administrator uploads a `.zip` file through the admin panel `upload_build_mod` THEN the system stores the zip as a single file under one of `mods/`, `resourcepacks/`, or `shaderpacks/` chosen by a content heuristic instead of treating it as a root-level archive.

1.2 WHEN the uploaded `.zip` contains top-level modpack folders such as `mods/`, `config/`, `shaderpacks/` THEN the system still routes it to a single subfolder based on `pack.mcmeta` / `shaders/` presence and ignores the multi-folder root layout of the archive.

1.3 WHEN a client launches the modpack and `sync_build_files` processes a build manifest entry pointing to such a `.zip` THEN the system downloads the zip as a regular file into the chosen subfolder and never extracts its contents into the modpack's root game directory.

1.4 WHEN the admin uploads a `.zip` whose internal structure is meant to merge into the modpack root THEN the system delivers an unusable artifact to clients: the inner `mods/x.jar`, `config/y.toml`, `shaderpacks/z.zip` files never appear at `mc_dir/mods/x.jar`, `mc_dir/config/y.toml`, `mc_dir/shaderpacks/z.zip`.

### Expected Behavior (Correct)

A `.zip` uploaded by the admin must be treated as an opaque archive of the modpack root. It must be uploaded as-is to a dedicated location that is not one of the runtime subfolders, and at launch time its contents must be extracted into the modpack's root game directory.

2.1 WHEN an administrator uploads a `.zip` file through the admin panel `upload_build_mod` THEN the system SHALL store the zip as a single file under a dedicated archive location in the build repository (e.g. `packs/<file>.zip`) that is distinct from `mods/`, `resourcepacks/`, `shaderpacks/`, `config/`, and `schematics/`.

2.2 WHEN the uploaded `.zip` contains any top-level layout (including `mods/`, `config/`, `shaderpacks/`, or any combination) THEN the system SHALL upload the zip verbatim without inspecting its contents to choose a destination subfolder.

2.3 WHEN a client launches the modpack and `sync_build_files` processes a build manifest entry that refers to such a `.zip` archive THEN the system SHALL download the archive and extract its contents into the modpack's root game directory (`mc_dir`) so that an inner path `foo/bar.ext` inside the zip lands at `mc_dir/foo/bar.ext`.

2.4 WHEN the admin-uploaded `.zip` is extracted on a client THEN the system SHALL NOT load the zip itself as a Minecraft mod, resource pack, or shader pack: only its extracted contents reach the modpack folders.

2.5 WHEN extraction is complete THEN the system SHALL ensure that files coming from the archive's inner `mods/` end up under `mc_dir/mods/`, files from `config/` end up under `mc_dir/config/`, files from `shaderpacks/` end up under `mc_dir/shaderpacks/`, and similarly for any other top-level folder present in the archive.

### Unchanged Behavior (Regression Prevention)

The fix must not alter how non-archive uploads or non-admin-archive build entries are handled. Mod jars, loose resource packs, loose shader packs, configs, and folder uploads must continue to work exactly as before.

3.1 WHEN an administrator uploads a single `.jar` file (e.g. a mod) through `upload_build_mod` THEN the system SHALL CONTINUE TO store it under `mods/` and clients SHALL CONTINUE TO download it into `mc_dir/mods/` without any extraction step.

3.2 WHEN an administrator uploads a single non-zip file other than a jar (e.g. `options.txt` or a `.toml` config) through `upload_build_mod` THEN the system SHALL CONTINUE TO place it according to its existing routing rules without any new extraction step.

3.3 WHEN an administrator uploads a directory through `upload_build_mod` (the existing `is_dir` branch) THEN the system SHALL CONTINUE TO zip the directory and route it to the appropriate subfolder using the existing `pack.mcmeta` / `shaders/` heuristic, and clients SHALL CONTINUE TO place the resulting zip in that subfolder without extracting it.

3.4 WHEN an administrator uploads a full modpack folder via `upload_modpack_build` THEN the system SHALL CONTINUE TO walk `mods/`, `config/`, `resourcepacks/`, `shaderpacks/`, `schematics/`, and `options.txt` and upload each file individually, with no behavioural change.

3.5 WHEN a client runs `sync_build_files` and the build manifest entry points to a non-archive file (e.g. a `.jar` under `mods/`, a file under `config/`, `resourcepacks/`, `shaderpacks/`, `schematics/`, or `options.txt`) THEN the system SHALL CONTINUE TO download it to its declared path and apply the existing user-owned-vs-mod sync semantics without extracting anything.

3.6 WHEN a client runs `sync_build_files` and the build manifest entry points to a `.zip` that already lives under one of the historical subfolders (`mods/`, `resourcepacks/`, `shaderpacks/`) — i.e. archives uploaded before this fix — THEN the system SHALL CONTINUE TO treat them as it does today (downloaded into that subfolder without extraction), so existing builds and clients do not regress.

3.7 WHEN the existing stale-file cleanup runs over `mc_dir/mods/` THEN the system SHALL CONTINUE TO remove only files whose paths are not present in the manifest's enabled set, with no change to which files are considered stale.
