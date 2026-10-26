//! Versioned data-only signature catalog. A signature selects a built-in validator;
//! it never establishes a recovered boundary or loads executable code.
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeSet, sync::OnceLock};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormatSpec {
    pub id: String,
    pub label: String,
    pub aliases: Vec<String>,
    pub family: String,
    pub enabled: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    version: u32,
    formats: Vec<Definition>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Definition {
    id: String,
    label: String,
    aliases: Vec<String>,
    family: String,
    enabled: bool,
    validator: String,
    max_bytes: u64,
    signatures: Vec<Signature>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Signature {
    offset: usize,
    hex: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Rule {
    pub offset: usize,
    pub magic: Vec<u8>,
    pub validator: String,
    pub max_bytes: u64,
}
struct Registry {
    formats: Vec<FormatSpec>,
    rules: Vec<(String, Rule)>,
}
fn bad() -> Error {
    Error::Invalid("Invalid or unsupported signature registry configuration".into())
}
fn id_ok(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 24
        && id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}
fn parse(text: &str) -> Result<Registry> {
    if text.len() > 65536 {
        return Err(bad());
    }
    let doc: Document = serde_json::from_str(text).map_err(|_| bad())?;
    if doc.version != 1 || doc.formats.is_empty() || doc.formats.len() > 64 {
        return Err(bad());
    }
    let mut ids = BTreeSet::new();
    let mut formats = Vec::new();
    let mut rules = Vec::new();
    for f in doc.formats {
        if !id_ok(&f.id)
            || !ids.insert(f.id.clone())
            || f.label.is_empty()
            || f.label.len() > 100
            || !id_ok(&f.family)
            || f.aliases.len() > 8
            || f.signatures.is_empty()
            || f.signatures.len() > 8
            || !(8..=1u64 << 46).contains(&f.max_bytes)
            || ![
                "jpeg", "png", "pdf", "gif", "zip-eocd", "rar", "bmp", "wav", "video",
            ]
            .contains(&f.validator.as_str())
        {
            return Err(bad());
        }
        for alias in &f.aliases {
            if !id_ok(alias) || !ids.insert(alias.clone()) {
                return Err(bad());
            }
        }
        for s in f.signatures {
            if s.offset > 1024
                || s.hex.is_empty()
                || s.hex.len() > 128
                || !s.hex.len().is_multiple_of(2)
            {
                return Err(bad());
            }
            let magic: Vec<u8> = s
                .hex
                .as_bytes()
                .chunks(2)
                .map(|pair| {
                    let h = (pair[0] as char).to_digit(16).ok_or_else(bad)?;
                    let l = (pair[1] as char).to_digit(16).ok_or_else(bad)?;
                    Ok((h * 16 + l) as u8)
                })
                .collect::<Result<_>>()?;
            rules.push((
                f.id.clone(),
                Rule {
                    offset: s.offset,
                    magic,
                    validator: f.validator.clone(),
                    max_bytes: f.max_bytes,
                },
            ));
        }
        formats.push(FormatSpec {
            id: f.id,
            label: f.label,
            aliases: f.aliases,
            family: f.family,
            enabled: f.enabled,
        });
    }
    if rules.len() > 256 {
        return Err(bad());
    }
    Ok(Registry { formats, rules })
}
fn registry() -> &'static Registry {
    static VALUE: OnceLock<Registry> = OnceLock::new();
    VALUE.get_or_init(|| {
        parse(include_str!("formats.json")).expect("checked embedded format registry")
    })
}
pub fn supported_formats() -> Vec<FormatSpec> {
    registry().formats.clone()
}
fn canonical(kind: &str) -> String {
    let lower = kind.to_ascii_lowercase();
    registry()
        .formats
        .iter()
        .find(|f| f.id == lower || f.aliases.contains(&lower))
        .map_or(lower, |f| f.id.clone())
}
pub fn matches_filter(kind: &str, types: &[String]) -> bool {
    if types.is_empty() {
        return true;
    }
    let kind = canonical(kind);
    types.iter().any(|t| {
        let t = canonical(t);
        t == kind || (t == "zip" && kind == "docx")
    })
}
pub(crate) fn rules(types: &[String]) -> Vec<Rule> {
    let mut out = Vec::new();
    for (id, rule) in &registry().rules {
        let format = registry()
            .formats
            .iter()
            .find(|f| &f.id == id)
            .expect("owned id");
        if format.enabled
            && (types.is_empty()
                || types.iter().any(|t| {
                    let t = canonical(t);
                    t == *id
                        || (format.family == "zip" && (t == "zip" || t == "docx"))
                        || (format.family == "video" && (t == "mp4" || t == "mov"))
                }))
            && !out.contains(rule)
        {
            out.push(rule.clone());
        }
    }
    out
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_has_aliases_and_specific_zip_subtype_filters() {
        assert_eq!(supported_formats().len(), 11);
        assert!(matches_filter("JPEG", &["jpg".into()]));
        assert!(matches_filter("jpg", &["jpeg".into()]));
        assert!(matches_filter("docx", &["zip".into()]));
        assert!(!matches_filter("zip", &["docx".into()]));
        assert_eq!(rules(&["docx".into()]).len(), 1);
    }
    #[test]
    fn registry_rejects_unknown_versions_handlers_and_unbounded_signatures() {
        let original = include_str!("formats.json");
        for altered in [
            original.replacen("\"version\": 1", "\"version\": 2", 1),
            original.replacen("\"jpeg\"", "\"../../plugin\"", 1),
            original.replacen("\"hex\":\"ffd8ff\"", "\"hex\":\"\"", 1),
            original.replacen("\"offset\":0", "\"offset\":65535", 1),
            original.replacen("\"id\":\"png\"", "\"id\":\"jpg\"", 1),
        ] {
            assert!(parse(&altered).is_err());
        }
    }
}
