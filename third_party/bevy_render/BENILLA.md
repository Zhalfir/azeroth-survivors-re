# benilla's `bevy_render` copy — what differs from upstream

Upstream: `bevy_render 0.18.1` (crates.io), identical except `src/render_resource/pipeline_cache.rs`:
the `disk_cache` module at its end, and its three call sites (`PipelineCache::new` initialises
it, both pipeline descriptors pass `cache: disk_cache::get()` instead of `cache: None`, and
`process_queue` calls `disk_cache::maybe_save`).

Reason: upstream never gives wgpu a pipeline cache, so a driver recompiles every pipeline on every
launch. On Android's Vulkan drivers that is long loading and a stutter when a variant is first
drawn. With `$BEVY_PIPELINE_CACHE_PATH` set (the Android launcher sets it) and the device offering
`wgpu::Features::PIPELINE_CACHE`, one cache seeded from that file serves every pipeline and is
written back as pipelines arrive. Without the variable, behaviour is upstream's.

Wired in through `[patch.crates-io]` in the workspace root. Verify the diff with:

    diff -r ~/.cargo/registry/src/*/bevy_render-0.18.1 third_party/bevy_render
