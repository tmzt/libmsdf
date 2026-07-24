# libmsdf

The wgpu SDF/MSDF rendering engine and typography stack, extracted from
matter-stream's render crates (`matterstream-common` / `-font` / `-ui-gpu`)
with all VM, skills, and card code pruned: SDF primitive instancing
(box / rounded-box / circle / line / Bézier stroke), MSDF text via font
atlases (baked at build time **and** generated at runtime by a WGSL compute
shader), rustybuzz shaping, and the `DrawList` contract that highbay_ui's
node-graph lowers into. Runs native and on wasm32 (WebGPU); includes the lo-fi
"pencil sketch" noise-distortion hook point (PLAN.md Phase 5).

Status: Phase 1 contract stubs; Phase 5 implements (no wgpu dependency yet).
