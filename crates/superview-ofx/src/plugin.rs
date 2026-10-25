//! The glue between Resolve and [`crate::warp`]: describe the effect, then warp each row.
//!
//! In Resolve the effect receives the whole timeline frame with the clip already scaled to fit
//! (set *Mismatched resolution* to *Scale entire image to fit*), so source and output have the
//! same bounds and the effect writes every column.

use std::panic::{AssertUnwindSafe, catch_unwind};

use ofx::*;

use crate::warp::{Mode, column_map, warp_row};

plugin_module!(
    "io.github.canaill51.superview",
    ApiVersion(1),
    PluginVersion(1, 0),
    SuperviewPlugin::default
);

const PARAM_MODE: &str = "mode";

#[derive(Default)]
struct SuperviewPlugin;

struct InstanceData {
    source_clip: ClipInstance,
    output_clip: ClipInstance,
    mode: ParamHandle<Int>,
}

impl Execute for SuperviewPlugin {
    /// A panic must not cross into the host: it would take Resolve down with it.
    fn execute(&mut self, plugin_context: &PluginContext, action: &mut Action) -> Result<Int> {
        match catch_unwind(AssertUnwindSafe(|| run(plugin_context, action))) {
            Ok(result) => result,
            Err(_) => {
                log::error!("Superview: the effect panicked; the frame is left as it was");
                FAILED
            }
        }
    }
}

fn run(_plugin_context: &PluginContext, action: &mut Action) -> Result<Int> {
    use Action::*;
    match *action {
        Describe(ref mut effect) => {
            let mut props: EffectDescriptor = effect.properties()?;
            props.set_grouping("Superview")?;
            props.set_label("Superview")?;
            props.set_short_label("Superview")?;
            props.set_long_label("Superview: widen 4:3 to 16:9")?;
            props.set_supported_pixel_depths(&[
                BitDepth::Byte,
                BitDepth::Short,
                BitDepth::Float,
            ])?;
            props.set_supported_contexts(&[ImageEffectContext::Filter])?;
            props.set_supports_tiles(false)?;
            // The host then hands over source and output of the same size, which `Render` relies on.
            props.set_supports_multi_resolution(false)?;
            // The fork reaches its registry through a global: one render at a time per instance.
            props.set_render_thread_safety(ImageEffectRender::InstanceSafe)?;
            OK
        }

        DescribeInContext(ref mut effect, _) => {
            effect
                .new_output_clip()?
                .set_supported_components(&[ImageComponent::RGBA])?;
            effect
                .new_simple_input_clip()?
                .set_supported_components(&[ImageComponent::RGBA])?;

            let mut params = effect.parameter_set()?;
            let mut mode = params.param_define_choice(PARAM_MODE)?;
            mode.set_label("Mode")?;
            mode.set_hint(
                "Widen: a 4:3 picture centred in a wider frame is stretched to fill it. \
                 Un-squeeze: the picture already fills the frame, stretched by the camera.",
            )?;
            mode.set_choices(&["Widen 4:3 to 16:9", "Un-squeeze"])?;
            mode.set_default(0)?;
            params
                .param_define_page("Main")?
                .set_children(&[PARAM_MODE])?;
            OK
        }

        CreateInstance(ref mut effect) => {
            let params = effect.parameter_set()?;
            let data = InstanceData {
                source_clip: effect.get_simple_input_clip()?,
                output_clip: effect.get_output_clip()?,
                mode: params.parameter(PARAM_MODE)?,
            };
            effect.set_instance_data(data)?;
            OK
        }

        DestroyInstance(_) => OK,

        Render(ref mut effect, ref in_args) => {
            let time = in_args.get_time()?;
            let window = in_args.get_render_window()?;
            let data: &mut InstanceData = effect.get_instance_data()?;
            let mode = match data.mode.get_value_at_time(time)? {
                0 => Mode::Widen,
                _ => Mode::Unsqueeze,
            };

            let source = data.source_clip.get_image(time)?;
            let output = data.output_clip.get_image_mut(time)?;
            let mut output = output.borrow_mut();

            let bounds = output.get_bounds()?;
            let from = source.get_bounds()?;
            if (from.x1, from.y1, from.x2, from.y2) != (bounds.x1, bounds.y1, bounds.x2, bounds.y2)
            {
                log::error!("Superview: the source and output frames differ in size");
                return FAILED;
            }
            let width = (bounds.x2 - bounds.x1).max(0) as usize;
            let height = (bounds.y2 - bounds.y1).max(0) as usize;
            if width == 0 || height == 0 {
                return OK;
            }
            let map = column_map(width, height, mode);
            let rows = window.y1.max(bounds.y1)..window.y2.min(bounds.y2);

            macro_rules! warp {
                ($pixel:ty) => {{
                    let src = source.get_descriptor::<$pixel>()?;
                    let mut tiles = output.get_tiles_mut::<$pixel>(1)?;
                    let Some(tile) = tiles.first_mut() else {
                        return FAILED;
                    };
                    for y in rows.clone() {
                        // The user scrolled on: the host no longer wants this frame, which is no error.
                        if effect.abort()? {
                            return OK;
                        }
                        if !warp_row(src.row(y), tile.row(y), &map) {
                            log::error!("Superview: a row does not have the width of the frame");
                            return FAILED;
                        }
                    }
                }};
            }
            match (output.get_pixel_depth()?, output.get_components()?) {
                (BitDepth::Float, ImageComponent::RGBA) => warp!(RGBAColourF),
                (BitDepth::Short, ImageComponent::RGBA) => warp!(RGBAColourS),
                (BitDepth::Byte, ImageComponent::RGBA) => warp!(RGBAColourB),
                _ => return FAILED,
            }
            OK
        }

        _ => REPLY_DEFAULT,
    }
}
