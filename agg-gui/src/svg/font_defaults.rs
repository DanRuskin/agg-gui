//! Generic SVG font-family selection helpers.
//!
//! `fontdb` starts with Windows-oriented generic family names.  This module
//! retargets those aliases to fonts that are actually present in the database
//! so default SVG parsing works on Linux CI, native desktops, and WASM hosts
//! that install app-owned font bytes.

pub(super) fn configure_generic_font_families(
    fontdb: &mut usvg::fontdb::Database,
    preferred_family: Option<&str>,
) {
    let loaded_families = fontdb
        .faces()
        .flat_map(|face| face.families.iter().map(|family| family.0.clone()))
        .collect::<Vec<_>>();
    if loaded_families.is_empty() {
        return;
    }

    let fallback = loaded_families[0].clone();
    let preferred = preferred_family
        .filter(|family| has_family(&loaded_families, family))
        .map(str::to_string);

    let serif = preferred.clone().unwrap_or_else(|| {
        pick_family(
            &loaded_families,
            &[
                "Times New Roman",
                "Liberation Serif",
                "DejaVu Serif",
                "Noto Serif",
            ],
        )
        .unwrap_or_else(|| fallback.clone())
    });
    let sans_serif = preferred.clone().unwrap_or_else(|| {
        pick_family(
            &loaded_families,
            &[
                "Arial",
                "Verdana",
                "Liberation Sans",
                "DejaVu Sans",
                "Noto Sans",
            ],
        )
        .unwrap_or_else(|| fallback.clone())
    });
    let monospace = preferred.unwrap_or_else(|| {
        pick_family(
            &loaded_families,
            &[
                "Courier New",
                "Cascadia Code",
                "Consolas",
                "Liberation Mono",
                "DejaVu Sans Mono",
                "Noto Sans Mono",
            ],
        )
        .unwrap_or(fallback)
    });

    fontdb.set_serif_family(serif);
    fontdb.set_sans_serif_family(sans_serif);
    fontdb.set_monospace_family(monospace);
}

fn pick_family(loaded_families: &[String], candidates: &[&str]) -> Option<String> {
    candidates
        .iter()
        .find(|candidate| has_family(loaded_families, candidate))
        .map(|family| (*family).to_string())
}

fn has_family(loaded_families: &[String], family: &str) -> bool {
    loaded_families
        .iter()
        .any(|loaded| loaded.eq_ignore_ascii_case(family))
}
