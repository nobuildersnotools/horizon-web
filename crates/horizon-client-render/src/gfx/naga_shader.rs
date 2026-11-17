//! Shaders the renderer builds as naga IR (resource and shader packs' GLSL, Horizon's own
//! WGSL reworked as IR), as wgpu takes them.

/// The browser's WGSL compilers enforce derivative uniformity, which naga doesn't: the
/// renderer's own shaders and packs' GLSL sample in branches on per-vertex values, where the
/// derivatives are the interpolated UVs' either way. The directive must open the module.
#[cfg(target_arch = "wasm32")]
const DIRECTIVE: &str = "diagnostic(off, derivative_uniformity);\n";

/// `module` for `ShaderModuleDescriptor::source`. Natively the IR itself. In the browser,
/// WGSL text opening with [`DIRECTIVE`]: wgpu would write the IR out itself, but naga's writer
/// drops the directive the renderer's sources open with, and wgpu unwraps the writer's
/// result, so a module naga can't write would abort the page; here that module fails to
/// compile instead.
pub(crate) fn naga_source(module: naga::Module) -> wgpu::ShaderSource<'static> {
    #[cfg(target_arch = "wasm32")]
    {
        web_wgsl(module)
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module))
    }
}

#[cfg(target_arch = "wasm32")]
fn web_wgsl(module: naga::Module) -> wgpu::ShaderSource<'static> {
    use naga::valid::{Capabilities, ValidationFlags, Validator};
    // Validated as wgpu validates it; an invalid module goes to wgpu as IR, which reports the
    // error as the module's without writing it.
    let Ok(info) = Validator::new(ValidationFlags::all(), Capabilities::all()).validate(&module)
    else {
        return wgpu::ShaderSource::Naga(std::borrow::Cow::Owned(module));
    };
    match naga::back::wgsl::write_string(&module, &info, naga::back::wgsl::WriterFlags::empty()) {
        Ok(text) => wgpu::ShaderSource::Wgsl(format!("{DIRECTIVE}{text}").into()),
        Err(e) => {
            log::warn!("Horizon renderer: a shader can't be written as WGSL: {e}");
            // Not WGSL: the module fails to compile, with the reason in its source.
            wgpu::ShaderSource::Wgsl(
                format!("// naga can't write this shader as WGSL: {e}\nunwritable_shader;\n")
                    .into(),
            )
        }
    }
}
