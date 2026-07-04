use super::{azlin_page, AzlinPage};

/// One funding-platform tile in the `.docs-card-grid`.
fn donation_card(href: &str, name: &str, description: &str) -> String {
    format!(
        r#"        <a class="docs-card" href="{href}" target="_blank" rel="noopener noreferrer">
          <h4>{name}</h4>
          <p>{description}</p>
        </a>
"#
    )
}

/// Generate the donate page (azlin docs shell). Funding platforms come from
/// .github/FUNDING.yml, passed in as `yaml_str`.
pub fn generate_donation_page(yaml_str: &str) -> anyhow::Result<String> {
    // Parse FUNDING.yml
    #[derive(serde_derive::Deserialize, Debug)]
    struct FundingConfig {
        github: Option<String>,
        ko_fi: Option<String>,
        liberapay: Option<String>,
        custom: Option<Vec<String>>,
    }

    let funding: FundingConfig = serde_yaml::from_str(yaml_str)?;

    let mut donation_cards = String::new();
    let mut github_sponsors_url = None;

    // GitHub Sponsors
    if let Some(github_user) = &funding.github {
        let url = format!("https://github.com/sponsors/{}", github_user);
        donation_cards.push_str(&donation_card(
            &url,
            "GitHub Sponsors",
            "One-time or recurring sponsorship through your GitHub account.",
        ));
        github_sponsors_url = Some(url);
    }

    // Ko-fi
    if let Some(kofi_user) = &funding.ko_fi {
        donation_cards.push_str(&donation_card(
            &format!("https://ko-fi.com/{}", kofi_user),
            "Ko-fi",
            "Quick one-time support - buy the maintainer a coffee.",
        ));
    }

    // Liberapay
    if let Some(liberapay_user) = &funding.liberapay {
        donation_cards.push_str(&donation_card(
            &format!("https://liberapay.com/{}", liberapay_user),
            "Liberapay",
            "Recurring donations via the non-profit, open-source platform.",
        ));
    }

    // Custom options
    if let Some(custom_options) = &funding.custom {
        for url in custom_options {
            let (service, description) = if url.contains("paypal.me") {
                ("PayPal", "One-time donation via PayPal.")
            } else if url.contains("wise.com") {
                ("Wise", "Direct bank transfer via Wise.")
            } else {
                ("Other", "Support the project via this platform.")
            };
            donation_cards.push_str(&donation_card(url, service, description));
        }
    }

    // The GitHub Sponsors CARD in the grid is the (only) GitHub CTA - a
    // separate .btn duplicated it ("Sponsor on GitHub twice" bug).
    let _ = &github_sponsors_url;

    let main_html = format!(
        r#"    <section class="docs-hero">
      <div class="container">
        <p class="docs-eyebrow">Support Azul</p>
        <h1>Donate</h1>
        <p class="docs-lede">Azul is an open-source GUI framework that relies on community
        support to continue development. Your contributions help maintain the project,
        implement new features, and keep resources available to everyone.</p>
      </div>
    </section>
    <section class="docs-body">
      <div class="container">
        <div class="docs-content">
          <p>Choose one of the options below to support the project:</p>
          <div class="docs-card-grid">
{donation_cards}          </div>
          <p>Thank you for considering supporting Azul! Every contribution helps the
          project grow.</p>
          <p>If you have any questions about donations, please reach out via
          <a href="https://github.com/fschutt/azul/issues">GitHub</a>.</p>
        </div>
      </div>
    </section>"#
    );

    Ok(azlin_page(
        &AzlinPage {
            title: "Support Azul".to_string(),
            active_nav: "donate",
            head_extra: String::new(),
            page_css: None,
            main_html,
        },
        true,
    ))
}
