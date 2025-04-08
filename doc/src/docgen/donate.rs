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
    
    let mut donation_options = String::new();
    
    // GitHub Sponsors
    if let Some(github_user) = &funding.github {
        donation_options.push_str(&format!(
            r#"<div class="donation-option">
                <h2>GitHub Sponsors</h2>
                <p>Support development directly through GitHub Sponsors.</p>
                <a href="https://github.com/sponsors/{}" class="donation-button github">
                    Sponsor on GitHub
                </a>
            </div>"#,
            github_user
        ));
    }
    
    // Ko-fi
    if let Some(kofi_user) = &funding.ko_fi {
        donation_options.push_str(&format!(
            r#"<div class="donation-option">
                <h2>Ko-fi</h2>
                <p>Buy me a coffee to keep development going.</p>
                <a href="https://ko-fi.com/{}" class="donation-button kofi">
                    Support on Ko-fi
                </a>
            </div>"#,
            kofi_user
        ));
    }
    
    // Liberapay
    if let Some(liberapay_user) = &funding.liberapay {
        donation_options.push_str(&format!(
            r#"<div class="donation-option">
                <h2>Liberapay</h2>
                <p>Support through Liberapay, an open source donation platform.</p>
                <a href="https://liberapay.com/{}" class="donation-button liberapay">
                    Donate on Liberapay
                </a>
            </div>"#,
            liberapay_user
        ));
    }
    
    // Custom options
    if let Some(custom_options) = &funding.custom {
        for url in custom_options {
            let (service, button_class) = if url.contains("paypal.me") {
                ("PayPal", "paypal")
            } else if url.contains("wise.com") {
                ("Wise", "wise")
            } else {
                ("Other", "other")
            };
            
            donation_options.push_str(&format!(
                r#"<div class="donation-option">
                    <h2>{}</h2>
                    <p>Direct payment through {}.</p>
                    <a href="{}" class="donation-button {}">
                        Donate via {}
                    </a>
                </div>"#,
                service, service, url, button_class, service
            ));
        }
    }
    
    // Get common head tags and sidebar
    let common_head_tags = crate::docgen::get_common_head_tags();
    let sidebar = crate::docgen::get_sidebar();
    
    // Additional CSS for the donation page
    let donation_css = r#"
        .donation-container {
            display: flex;
            flex-wrap: wrap;
            gap: 2rem;
            margin-top: 2rem;
        }
        
        .donation-option {
            background-color: #f5f7fa;
            border-radius: 10px;
            box-shadow: 0 2px 10px rgba(0, 0, 0, 0.1);
            padding: 2rem;
            width: 300px;
            transition: transform 0.2s, box-shadow 0.2s;
        }
        
        .donation-option:hover {
            transform: translateY(-5px);
            box-shadow: 0 5px 15px rgba(0, 0, 0, 0.2);
        }
        
        .donation-option h2 {
            color: #004e92;
            margin-bottom: 1rem;
            font-size: 1.5rem;
        }
        
        .donation-option p {
            margin-bottom: 1.5rem;
            color: #555;
            line-height: 1.4;
        }
        
        .donation-button {
            display: inline-block;
            background-color: #004e92;
            color: white;
            padding: 0.75rem 1.5rem;
            border-radius: 5px;
            text-decoration: none;
            font-weight: bold;
            transition: background-color 0.2s;
            text-align: center;
            width: 100%;
            box-sizing: border-box;
        }
        
        .donation-button:hover {
            background-color: #003366;
        }
        
        .donation-intro {
            max-width: 800px;
            line-height: 1.6;
            font-size: 1.2rem;
        }
        
        .github { background-color: #24292e; }
        .github:hover { background-color: #1a1e22; }
        
        .kofi { background-color: #29abe0; }
        .kofi:hover { background-color: #2180ab; }
        
        .liberapay { background-color: #f6c915; color: #1a171b; }
        .liberapay:hover { background-color: #e0b50e; }
        
        .paypal { background-color: #003087; }
        .paypal:hover { background-color: #001e53; }
        
        .wise { background-color: #9fe870; color: #1a171b; }
        .wise:hover { background-color: #8ad057; }
        
        @media (max-width: 768px) {
            .donation-container {
                flex-direction: column;
                align-items: center;
            }
            
            .donation-option {
                width: 100%;
                max-width: 300px;
            }
            
            .donation-intro {
                padding: 0 1rem;
            }
        }
    "#;
    
    // Generate the full HTML page
    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
  <title>Support Azul GUI Framework Development</title>
  {common_head_tags}
  <style>
    {donation_css}
  </style>
</head>
<body>
  <div class="center">
    <aside>
      <header>
        <h1 style="display:none;">Azul GUI Framework</h1>
        <a href="{html_root}">
          <img src="{html_root}/logo.svg">
        </a>
      </header>
      <nav>
        {sidebar}
      </nav>
    </aside>
    <main>
      <h1>Support Azul Development</h1>
      
      <div class="donation-intro">
        <p>Azul is an open-source GUI framework that relies on community support to continue development. 
        Your contributions help maintain the project, implement new features, and keep resources available to everyone.</p>
        <p>Choose one of the options below to support the project:</p>
      </div>
      
      <div class="donation-container">
        {donation_options}
      </div>
      
      <div class="donation-intro" style="margin-top: 2rem;">
        <p>Thank you for considering supporting Azul! Every contribution helps the project grow.</p>
        <p>If you have any questions about donations, please reach out via 
        <a href="https://github.com/fschutt/azul/issues">GitHub</a> or 
        <a href="https://discord.gg/V96ZGKqQvn">Discord</a>.</p>
      </div>
    </main>
  </div>
  <script async type="text/javascript" src="{html_root}/prism_code_highlighter.js"></script>
</body>
</html>"#,
        common_head_tags = common_head_tags,
        donation_css = donation_css,
        html_root = crate::docgen::HTML_ROOT,
        sidebar = sidebar,
        donation_options = donation_options
    );
    
    Ok(html)
}