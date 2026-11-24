//! GPU texture cache for Icons8 Liquid Glass default file and folder icons.
//!
//! Pre-decodes embedded PNG assets into Arc<RenderImage> with GPUI-expected
//! straight BGRA channel ordering, enabling zero-allocation instant rendering
//! in both high-performance custom virtual elements (DetailsElement, GridSurfaceElement)
//! and standard GPUI views (miller, list, inspector, etc.).

use std::collections::HashMap;
use std::sync::Arc;

use atlas_model::file_entry::FileKind;
use gpui::RenderImage;

use crate::assets::liquid_glass_filetype;
use crate::icon_theme::MultiResIcon;
use crate::ui::icons::{file_type_icon_kind, FileTypeIcon};

const ALL_ICONS: &[FileTypeIcon] = &[
    FileTypeIcon::Folder,
    FileTypeIcon::File,
    FileTypeIcon::Document,
    FileTypeIcon::Pdf,
    FileTypeIcon::Image,
    FileTypeIcon::Audio,
    FileTypeIcon::Video,
    FileTypeIcon::Code,
    FileTypeIcon::Python,
    FileTypeIcon::Archive,
    FileTypeIcon::Presentation,
];

pub(crate) struct LiquidGlassCache {
    textures: HashMap<FileTypeIcon, Arc<MultiResIcon>>,
    /// Same art with darkened ink: the white glass vanishes on light surfaces.
    light_textures: HashMap<FileTypeIcon, Arc<MultiResIcon>>,
    fallback: Arc<MultiResIcon>,
}

impl LiquidGlassCache {
    pub(crate) fn new() -> Self {
        let mut textures = HashMap::with_capacity(ALL_ICONS.len());
        let mut light_textures = HashMap::with_capacity(ALL_ICONS.len());

        for &kind in ALL_ICONS {
            let path = kind.path();
            if let Some(bytes) = liquid_glass_filetype(path) {
                if let Some(tex) =
                    decode_png_to_multi(bytes, [1.0; 3], AlphaMap::Gain(DARK_SURFACE_ALPHA))
                {
                    textures.insert(kind, tex);
                }
                if let Some(tex) = decode_png_to_multi(
                    bytes,
                    LIGHT_SURFACE_INK,
                    AlphaMap::Curve(LIGHT_SURFACE_ALPHA_CURVE),
                ) {
                    light_textures.insert(kind, tex);
                }
            }
        }

        let fallback = textures
            .get(&FileTypeIcon::File)
            .cloned()
            .unwrap_or_else(|| Arc::new(MultiResIcon::from_single(create_dummy_texture())));

        Self {
            textures,
            light_textures,
            fallback,
        }
    }

    #[inline]
    pub(crate) fn multi_for(&self, icon: FileTypeIcon) -> Arc<MultiResIcon> {
        self.textures
            .get(&icon)
            .cloned()
            .unwrap_or_else(|| self.fallback.clone())
    }

    /// Largest tier; kept for callers that want a single texture.
    #[inline]
    pub(crate) fn texture_for(&self, icon: FileTypeIcon) -> Arc<RenderImage> {
        self.multi_for(icon).full.clone()
    }

    #[inline]
    pub(crate) fn texture_for_entry(
        &self,
        kind: &FileKind,
        extension: Option<&str>,
    ) -> Arc<RenderImage> {
        let icon_kind = file_type_icon_kind(kind, extension);
        self.texture_for(icon_kind)
    }

    /// Variant for light surfaces (darkened ink so the glass stays visible).
    #[inline]
    pub(crate) fn multi_for_entry_on(
        &self,
        kind: &FileKind,
        extension: Option<&str>,
        light_surface: bool,
    ) -> Arc<MultiResIcon> {
        let icon_kind = file_type_icon_kind(kind, extension);
        if light_surface {
            if let Some(tex) = self.light_textures.get(&icon_kind) {
                return tex.clone();
            }
        }
        self.multi_for(icon_kind)
    }
}

/// The art is pure white at ~50% alpha: crisp on black, gray-on-gray on the
/// mid-tone surfaces of Graphite/Nord/glass. Denser glass keeps it legible.
const DARK_SURFACE_ALPHA: f32 = 1.4;
/// Per-channel RGB ink for the light-surface variant: the white glass becomes
/// a cool slate. A uniform gray scale read muddy on every light theme.
const LIGHT_SURFACE_INK: [f32; 3] = [0.42, 0.48, 0.57];
/// Light variant alpha = alpha^CURVE: lifts the faint folder front so the
/// gradient stays soft instead of a dark tab fading into a gray slab.
const LIGHT_SURFACE_ALPHA_CURVE: f32 = 0.6;

#[derive(Clone, Copy)]
enum AlphaMap {
    Gain(f32),
    Curve(f32),
}

/// Edge lengths of the pre-filtered tiers, matching `MultiResIcon::for_size`.
const TIERS: [u32; 4] = [32, 64, 128, 256];

/// Decode raw PNG bytes into mip tiers of straight-BGRA textures, scaling RGB per
/// channel by `ink` (1.0 = original art) and remapping alpha.
///
/// Resamples the original premultiplied art first with CatmullRom (avoiding
/// Lanczos3 ringing halos and avoiding magnifying post-curve stepped alpha),
/// then applies the ink/alpha curve and edge color bleed so GPU bilinear
/// sampling produces clean, fringe-free rounded silhouettes.
fn decode_png_to_multi(bytes: &[u8], ink: [f32; 3], alpha: AlphaMap) -> Option<Arc<MultiResIcon>> {
    let mut img = image::load_from_memory(bytes).ok()?.to_rgba8();

    // Premultiply original alpha before resampling so transparent pixels never bleed.
    for px in img.pixels_mut() {
        let [r, g, b, a] = px.0;
        let af = f32::from(a) / 255.0;
        px.0 = [
            (f32::from(r) * af).round() as u8,
            (f32::from(g) * af).round() as u8,
            (f32::from(b) * af).round() as u8,
            a,
        ];
    }

    let tier = |edge: u32| -> Option<Arc<RenderImage>> {
        let resized =
            image::imageops::resize(&img, edge, edge, image::imageops::FilterType::CatmullRom);
        let mut raw = resized.into_raw();
        for px in raw.chunks_exact_mut(4) {
            let [pr, pg, pb, pa] = [px[0], px[1], px[2], px[3]];
            if pa == 0 {
                px.copy_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            let src_a = f32::from(pa) / 255.0;
            let un = |v: u8, scale: f32| {
                ((f32::from(v) / src_a) * scale.min(1.0))
                    .clamp(0.0, 255.0)
                    .round() as u8
            };
            let out_a = match alpha {
                AlphaMap::Gain(gain) => src_a * gain,
                AlphaMap::Curve(exp) => src_a.powf(exp),
            }
            .clamp(0.0, 1.0);
            let out_a_u8 = (out_a * 255.0).round() as u8;
            if out_a_u8 == 0 {
                px.copy_from_slice(&[0, 0, 0, 0]);
            } else {
                // R<->B swap: GPUI expects straight BGRA.
                px[0] = un(pb, ink[2]);
                px[1] = un(pg, ink[1]);
                px[2] = un(pr, ink[0]);
                px[3] = out_a_u8;
            }
        }
        super::icon_loader::bleed_straight_edges(&mut raw, edge as usize);
        let out = image::RgbaImage::from_raw(edge, edge, raw)?;
        Some(Arc::new(RenderImage::new(vec![image::Frame::new(out)])))
    };

    Some(Arc::new(MultiResIcon {
        small: tier(TIERS[0])?,
        medium: tier(TIERS[1])?,
        large: tier(TIERS[2])?,
        full: tier(TIERS[3])?,
    }))
}

fn create_dummy_texture() -> Arc<RenderImage> {
    let buf = image::RgbaImage::from_raw(1, 1, vec![128, 128, 128, 255]).expect("1x1 buffer");
    Arc::new(RenderImage::new(vec![image::Frame::new(buf)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_all_eleven_liquid_glass_icons() {
        let cache = LiquidGlassCache::new();
        assert_eq!(
            cache.textures.len(),
            11,
            "All 11 Liquid Glass PNGs must decode"
        );

        for &kind in ALL_ICONS {
            let tex = cache.texture_for(kind);
            let sz = tex.size(0);
            assert!(
                sz.width.0 > 0 && sz.height.0 > 0,
                "Icon {:?} has invalid dimensions",
                kind
            );
        }

        // Test entry resolution
        let folder_tex = cache.texture_for_entry(&FileKind::Dir, None);
        assert_eq!(
            folder_tex.size(0),
            cache.texture_for(FileTypeIcon::Folder).size(0)
        );

        let code_tex = cache.texture_for_entry(&FileKind::File, Some("rs"));
        assert_eq!(
            code_tex.size(0),
            cache.texture_for(FileTypeIcon::Code).size(0)
        );
    }
}
