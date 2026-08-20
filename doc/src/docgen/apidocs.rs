use std::collections::{BTreeMap, BTreeSet};

use super::HTML_ROOT;
use crate::{
    api::{ApiData, ClassData, VersionData},
    utils::{
        analyze::{
            analyze_type, enum_is_union, has_recursive_destructor, is_primitive_arg,
            search_for_class_by_class_name,
        },
        string::format_doc_lines,
    },
};

/// How many types the "Commonly used" row offers.
const COMMON_LIMIT: usize = 30;

/// Generate API documentation HTML for a specific version.
///
/// Rendered in the azlin docs shell (see `docgen::azlin_page`); the API
/// listing styles live in `templates/docs-api.css` - NO inline CSS here.
pub fn generate_api_html(api_data: &ApiData, version: &str) -> String {
    let version_data = api_data.get_version(version).unwrap();

    let title = format!("API v{version}");
    let jump = quick_jump(&version_data);
    let content = generate_api_content(&version_data);
    let prism_script = crate::docgen::get_prism_script();
    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::Api);
    let details_script = reveal_script();

    let main_html = format!(
        r#"<section class="docs-hero">
      <div class="container">
        <h1>{title}</h1>
      </div>
    </section>
    <section class="docs-body">
      <div class="container">
        <div class="docs-layout">
          <div class="docs-content docs-wide">
{jump}
            <div id="api">
            {content}
            </div>
            <p class="api-backlink"><a href="{HTML_ROOT}/api">Back to API index</a></p>
          </div>
          <aside class="docs-search-rail">
            <div id="azul-search-mount" data-azs-inline></div>
          </aside>
        </div>
      </div>
    </section>"#
    );

    let page = crate::docgen::AzlinPage {
        title,
        active_nav: "api",
        head_extra: format!("{prism_script}\n{search_script}\n{details_script}"),
        page_css: Some(concat!(
            include_str!("../../templates/docs-api.css"),
            include_str!("../../templates/docs-guide.css"),
        )),
        main_html,
    };

    // The old shell hardcoded linked (non-inlined) shared CSS for API pages
    // (get_common_head_tags(false)); keep that behavior - the family
    // stylesheet above is inlined by the shell either way.
    crate::docgen::azlin_page(&page, false)
}

// ===========================================================================
// Quick jump
// ===========================================================================

/// The two rows above the listing: the types the guide actually leans on, and
/// one link per module.
///
/// Lives OUTSIDE `#api` on purpose - the listing resets links and paragraphs
/// to the mono face, which would repaint these buttons.
fn quick_jump(version_data: &VersionData) -> String {
    let mut out = String::from("            <section class=\"api-jump\">\n");

    let common = commonly_used(version_data);
    if !common.is_empty() {
        out.push_str("              <h2>Commonly used</h2>\n              <div \
                      class=\"guide-links\">\n");
        for name in &common {
            out.push_str(&format!(
                "                <a class=\"guide-link\" href=\"#st.{name}\">{name}</a>\n"
            ));
        }
        out.push_str("              </div>\n");
    }

    out.push_str("              <h2>Modules</h2>\n              <div class=\"guide-links\">\n");
    for module_name in version_data.api.keys() {
        out.push_str(&format!(
            "                <a class=\"guide-link\" \
             href=\"#m.{module_name}\">{module_name}</a>\n"
        ));
    }
    out.push_str("              </div>\n            </section>\n");
    out
}

/// The API types the guide names, ranked by how many CHAPTERS name them.
///
/// Document frequency, not raw count: a chapter that writes `Dom` forty times
/// is one chapter that needs `Dom`, and raw counts would rank whatever the
/// longest page happens to repeat. The curated `default-search-keys` of each
/// page count as a mention, which is how a type a chapter is *about* ranks
/// even when the prose spells it in a code sample.
fn commonly_used(version_data: &VersionData) -> Vec<String> {
    let mut classes: BTreeSet<&str> = BTreeSet::new();
    for module in version_data.api.values() {
        for class_name in module.classes.keys() {
            classes.insert(class_name.as_str());
        }
    }

    let guides = crate::docgen::guide::get_guide_list();
    let mut freq: BTreeMap<&str, usize> = BTreeMap::new();
    for guide in &guides {
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for key in &guide.default_search_keys {
            // `Dom` and `Dom.add_callback` both count for `Dom`.
            let head = key.split('.').next().unwrap_or("");
            if let Some(name) = classes.get(head) {
                seen.insert(name);
            }
        }
        for word in identifiers(&guide.content) {
            if let Some(name) = classes.get(word) {
                seen.insert(name);
            }
        }
        for name in seen {
            *freq.entry(name).or_default() += 1;
        }
    }

    let mut ranked: Vec<(&str, usize)> = freq.into_iter().collect();
    // Frequency first, then alphabetical so the row is stable between builds.
    ranked.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    ranked
        .into_iter()
        .take(COMMON_LIMIT)
        .map(|(name, _)| name.to_string())
        .collect()
}

/// Every CamelCase-ish word in the text. Intersecting these with the real
/// class names is cheaper and more accurate than searching the text once per
/// class (2000 classes x 80 chapters of substring scans).
fn identifiers(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| w.len() >= 3 && w.starts_with(|c: char| c.is_ascii_uppercase()))
}

// ===========================================================================
// The listing
// ===========================================================================

/// A type reference: the class name links to its entry when the API declares
/// it, and the pointer/reference decoration around it is preserved.
///
/// The three-branch dance (primitive / known class / unknown) was written out
/// eight times in this file; every call site now shares one.
fn render_type(version_data: &VersionData, ty: &str) -> String {
    let (prefix, type_name, suffix) = analyze_type(ty);
    if is_primitive_arg(&type_name) {
        return ty.to_string();
    }
    match search_for_class_by_class_name(version_data, &type_name) {
        Some((_, class_name)) => {
            format!("{prefix}<a href=\"#st.{type_name}\">{class_name}</a>{suffix}")
        }
        None => ty.to_string(),
    }
}

/// `1 variant` / `12 variants`.
fn count_label(n: usize, singular: &str) -> String {
    if n == 1 {
        format!("1 {singular}")
    } else {
        format!("{n} {singular}s")
    }
}

/// A collapsible group inside a class. Open unless told otherwise - the one
/// group that starts closed is an enum's variant list, which for the bigger
/// enums is a hundred lines of names between the reader and the next type.
fn group(summary: &str, body: &str, open: bool, extra_class: &str) -> String {
    if body.is_empty() {
        return String::new();
    }
    let open_attr = if open { " open" } else { "" };
    format!(
        "<details class=\"api-group{extra_class}\"{open_attr}><summary>{summary}</summary>\
         <div class=\"api-group-body\">{body}</div></details>"
    )
}

fn generate_api_content(version_data: &VersionData) -> String {
    let mut html = String::new();
    html.push_str("<ul>\n");

    for (module_name, module) in &version_data.api {
        let mut body = String::new();
        if let Some(doc) = &module.doc {
            body.push_str(&format!("<p class=\"m doc\">{}</p>", format_doc_lines(doc)));
        }
        body.push_str("<ul>");
        for (class_name, class_data) in &module.classes {
            body.push_str(&render_class(version_data, class_name, class_data));
        }
        body.push_str("</ul>");

        html.push_str(&format!("<li class=\"m\">"));
        html.push_str(&format!(
            "<details class=\"api-mod\" id=\"m.{module_name}\" open>\
             <summary><h3>mod {module_name}</h3>\
             <span class=\"api-n\">{}</span></summary>{body}</details>",
            count_label(module.classes.len(), "type"),
        ));
        html.push_str("</li>");
    }

    html.push_str("</ul>");
    html
}

/// One class: the heading is the summary, everything else is a group under it.
fn render_class(version_data: &VersionData, class_name: &str, class_data: &ClassData) -> String {
    let is_boxed_object = class_data.is_boxed_object;
    let treat_external_as_ptr = class_data.external.is_some() && is_boxed_object;
    let class_has_custom_destructor = class_data.custom_destructor.unwrap_or(false);
    let class_has_recursive_destructor = has_recursive_destructor(version_data, class_data);

    let destructor_warning = if class_has_custom_destructor
        || treat_external_as_ptr
        || class_has_recursive_destructor
    {
        "&nbsp;<span class=\"chd\">has destructor</span>"
    } else {
        ""
    };

    // What kind of thing is this, and what does its own body look like?
    let (li_class, kind_class, keyword, body) = if let Some(enum_fields) = &class_data.enum_fields {
        let keyword = if enum_is_union(enum_fields) {
            "union enum"
        } else {
            "enum"
        };
        let mut rows = String::new();
        let mut count = 0usize;
        for variant_map in enum_fields {
            for (variant_name, variant_data) in variant_map {
                count += 1;
                if let Some(doc) = &variant_data.doc {
                    rows.push_str(&format!("<p class=\"v doc\">{}</p>", format_doc_lines(doc)));
                }
                let id = format!("v.{class_name}.{variant_name}");
                match &variant_data.r#type {
                    Some(variant_type) => rows.push_str(&format!(
                        "<p class=\"f\" id=\"{id}\">{variant_name}({})</p>",
                        render_type(version_data, variant_type)
                    )),
                    None => rows.push_str(&format!("<p class=\"f\" id=\"{id}\">{variant_name}</p>")),
                }
            }
        }
        // The one group that starts CLOSED: some of these run to a hundred
        // variants, and a reader scrolling for the next type should not have
        // to scroll through all of them. Search still lands inside - the
        // reveal script opens whatever ancestor is holding the anchor.
        let body = group(&count_label(count, "variant"), &rows, false, " api-variants");
        ("st e pbi", "api-enum", keyword, body)
    } else if let Some(struct_fields) = &class_data.struct_fields {
        let mut rows = String::new();
        let mut count = 0usize;
        for field_map in struct_fields {
            for (field_name, field_data) in field_map {
                count += 1;
                if let Some(doc) = &field_data.doc {
                    rows.push_str(&format!("<p class=\"f doc\">{}</p>", format_doc_lines(doc)));
                }
                rows.push_str(&format!(
                    "<p class=\"f\" id=\"f.{class_name}.{field_name}\">{field_name}: {}</p>",
                    render_type(version_data, &field_data.r#type)
                ));
            }
        }
        let body = group(&count_label(count, "field"), &rows, true, "");
        ("st s pbi", "api-struct", "struct", body)
    } else if let Some(callback_typedef) = &class_data.callback_typedef {
        let mut rows = String::new();
        for arg in &callback_typedef.fn_args {
            if let Some(doc) = &arg.doc {
                rows.push_str(&format!("<p class=\"arg doc\">{}</p>", format_doc_lines(doc)));
            }
            let (_, type_name, _) = analyze_type(&arg.r#type);
            let ref_prefix = arg.ref_kind.to_rust_prefix();
            if is_primitive_arg(&type_name) {
                rows.push_str(&format!("<p class=\"f\">arg {type_name}</p>"));
            } else {
                rows.push_str(&format!(
                    "<p class=\"fnty arg\">arg {ref_prefix} {}</p>",
                    render_type(version_data, &arg.r#type)
                ));
            }
        }
        if let Some(returns) = &callback_typedef.returns {
            if let Some(doc) = &returns.doc {
                rows.push_str(&format!("<p class=\"ret doc\">{}</p>", format_doc_lines(doc)));
            }
            rows.push_str(&format!(
                "<p class=\"fnty ret\">-&gt;&nbsp;{}</p>",
                render_type(version_data, &returns.r#type)
            ));
        }
        let body = group("signature", &rows, true, "");
        ("pbi fnty", "api-fnty", "fnptr", body)
    } else if class_data.constructors.is_some() || class_data.functions.is_some() {
        // Methods only - no fields to show, but the class still gets an entry.
        ("st s pbi", "api-struct", "struct", String::new())
    } else {
        return String::new();
    };

    let mut inner = String::new();
    if let Some(doc) = &class_data.doc {
        inner.push_str(&format!("<p class=\"class doc\">{}</p>", format_doc_lines(doc)));
    }
    inner.push_str(&body);

    if let Some(constructors) = &class_data.constructors {
        let mut rows = String::from("<ul>");
        for (ctor_name, ctor) in constructors {
            rows.push_str(&render_member(
                version_data,
                class_name,
                ctor_name,
                ctor,
                "cn",
                "constructor",
            ));
        }
        rows.push_str("</ul>");
        inner.push_str(&group(
            &count_label(constructors.len(), "constructor"),
            &rows,
            true,
            "",
        ));
    }

    if let Some(functions) = &class_data.functions {
        let mut rows = String::from("<ul>");
        for (fn_name, function) in functions {
            rows.push_str(&render_member(
                version_data,
                class_name,
                fn_name,
                function,
                "fn",
                "fn",
            ));
        }
        rows.push_str("</ul>");
        inner.push_str(&group(
            &count_label(functions.len(), "method"),
            &rows,
            true,
            "",
        ));
    }

    format!(
        "<li class=\"{li_class}\">\
         <details class=\"api-class {kind_class}\" id=\"st.{class_name}\" open>\
         <summary><h4>{keyword} {class_name}</h4>{destructor_warning}</summary>\
         {inner}</details></li>"
    )
}

/// A constructor or a method: signature rows under a linkable header.
fn render_member(
    version_data: &VersionData,
    class_name: &str,
    member_name: &str,
    member: &crate::api::FunctionData,
    css_class: &str,
    keyword: &str,
) -> String {
    let mut out = String::new();
    if let Some(doc) = &member.doc {
        out.push_str(&format!(
            "<p class=\"{css_class} doc\">{}</p>",
            format_doc_lines(doc)
        ));
    }
    out.push_str(&format!(
        "<li class=\"{css_class}\" id=\"{class_name}.{member_name}\">\
         <p>{keyword} <a href=\"#{class_name}.{member_name}\">{member_name}</a>:</p><ul>"
    ));

    // `self`, when the API declares one, reads first.
    let mut self_arg = String::new();
    for arg_map in &member.fn_args {
        for (arg_name, arg_type) in arg_map {
            if arg_name == "self" {
                self_arg = match arg_type.as_str() {
                    "value" => "self".to_string(),
                    "ref" => "&self".to_string(),
                    "refmut" => "&mut self".to_string(),
                    _ => String::new(),
                };
            }
        }
    }
    if !self_arg.is_empty() {
        out.push_str(&format!("<li><p class=\"arg\">{self_arg}</p></li>"));
    }

    for arg_map in &member.fn_args {
        for (arg_name, arg_type) in arg_map {
            if arg_name == "self" {
                continue;
            }
            out.push_str(&format!(
                "<li><p class=\"arg\">arg {arg_name}: {}</p></li>",
                render_type(version_data, arg_type)
            ));
        }
    }

    match &member.returns {
        Some(returns) => {
            out.push_str("<li>");
            if let Some(doc) = &returns.doc {
                out.push_str(&format!("<p class=\"ret doc\">{}</p>", format_doc_lines(doc)));
            }
            out.push_str(&format!(
                "<p class=\"{css_class} ret\">-&gt;&nbsp;{}</p>",
                render_type(version_data, &returns.r#type)
            ));
            out.push_str("</li>");
        }
        // A constructor with no declared return builds its own class.
        None if css_class == "cn" => out.push_str(&format!(
            "<li><p class=\"ret\">-&gt;&nbsp;<a href=\"#st.{class_name}\">{class_name}</a></p></li>"
        )),
        None => {}
    }

    out.push_str("</ul></li>");
    out
}

/// Deep links have to survive the collapsing.
///
/// A `<details>` that is closed hides its content from layout, so the browser
/// scrolls to nothing when a link points inside one. This opens every
/// `<details>` on the path to the target (the target included) and re-scrolls,
/// which is what makes a search hit on an enum variant land on the variant.
/// Printing opens everything, since a closed section prints as its summary.
fn reveal_script() -> String {
    r#"<script>
(function () {
  function reveal() {
    var id = decodeURIComponent((location.hash || '').slice(1));
    if (!id) return;
    var el = document.getElementById(id);
    if (!el) return;
    for (var n = el; n; n = n.parentElement) {
      if (n.tagName === 'DETAILS') n.open = true;
    }
    el.scrollIntoView({ block: 'start' });
  }
  window.addEventListener('hashchange', reveal);
  if (document.readyState !== 'loading') reveal();
  else document.addEventListener('DOMContentLoaded', reveal);

  var reopened = [];
  window.addEventListener('beforeprint', function () {
    reopened = [];
    document.querySelectorAll('#api details:not([open])').forEach(function (d) {
      reopened.push(d);
      d.open = true;
    });
  });
  window.addEventListener('afterprint', function () {
    reopened.forEach(function (d) { d.open = false; });
    reopened = [];
  });
})();
</script>"#
        .to_string()
}

/// Generate a combined API index page (version selector).
pub fn generate_api_index(api_data: &ApiData) -> String {
    let title = format!("Select API version");

    // Version selector: same links as before ({HTML_ROOT}/api/<version>),
    // rendered as azlin docs cards. Ordering = get_sorted_versions, unchanged.
    let mut content = String::new();
    for version in api_data.get_sorted_versions() {
        content.push_str(&format!(
            // The version IS the destination, so the whole card is the link -
            // same treatment as the releases overview.
            "<a class=\"guide-card-btn\" href=\"{}/api/{}\">v{}</a>\n",
            HTML_ROOT, version, version
        ));
    }

    let prism_script = crate::docgen::get_prism_script();
    let search_script = crate::docgen::get_search_init(crate::docgen::PageKind::Api);

    let main_html = format!(
        r#"<section class="docs-hero">
      <div class="container">
        <h1>{title}</h1>
      </div>
    </section>
    <section class="docs-body">
      <div class="container">
        <div class="docs-content docs-wide">
          <div class="guide-grid api-version-grid">
          {content}
          </div>
        </div>
      </div>
    </section>"#
    );

    let page = crate::docgen::AzlinPage {
        title,
        active_nav: "api",
        head_extra: format!("{prism_script}\n{search_script}"),
        page_css: Some(concat!(
            include_str!("../../templates/docs-api.css"),
            include_str!("../../templates/docs-guide.css"),
        )),
        main_html,
    };

    crate::docgen::azlin_page(&page, false)
}
