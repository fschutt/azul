#!/usr/bin/env python3
"""
Link checker script for azul.rs website.
Validates all internal links in the generated deploy folder.

Usage:
    python check_links.py [deploy_dir]

If no directory is specified, defaults to ../target/deploy
"""

import os
import sys
import re
from pathlib import Path
from urllib.parse import urlparse, unquote
from html.parser import HTMLParser
from typing import Set, Dict, List, Tuple


class LinkExtractor(HTMLParser):
    """Extract all links from HTML content."""
    
    def __init__(self):
        super().__init__()
        self.links: List[Tuple[str, str]] = []  # (attr_name, url)
        
    def handle_starttag(self, tag, attrs):
        attrs_dict = dict(attrs)
        
        # Check href attributes (a, link tags)
        if 'href' in attrs_dict:
            self.links.append(('href', attrs_dict['href']))
            
        # Check src attributes (img, script tags)
        if 'src' in attrs_dict:
            self.links.append(('src', attrs_dict['src']))
            
        # Check data-src attributes (lazy loading)
        if 'data-src' in attrs_dict:
            self.links.append(('data-src', attrs_dict['data-src']))


def get_all_html_files(deploy_dir: Path) -> List[Path]:
    """Get all HTML files in the deploy directory."""
    html_files = []
    for root, dirs, files in os.walk(deploy_dir):
        for file in files:
            if file.endswith('.html'):
                html_files.append(Path(root) / file)
    return html_files


def extract_links_from_file(html_file: Path) -> List[Tuple[str, str]]:
    """Extract all links from an HTML file."""
    try:
        content = html_file.read_text(encoding='utf-8')
        parser = LinkExtractor()
        parser.feed(content)
        return parser.links
    except Exception as e:
        print(f"  Warning: Could not parse {html_file}: {e}")
        return []


def is_internal_link(url: str) -> bool:
    """Check if a URL is an internal link that should be validated."""
    if not url:
        return False
        
    # Skip external links
    parsed = urlparse(url)
    if parsed.scheme in ('http', 'https', 'mailto', 'tel'):
        # Only check azul.rs links
        if parsed.netloc and parsed.netloc != 'azul.rs':
            return False
        # azul.rs links should be treated as internal
        if parsed.netloc == 'azul.rs':
            return True
            
    # Skip anchors-only links
    if url.startswith('#'):
        return False
        
    # Skip javascript: and data: URLs
    if url.startswith('javascript:') or url.startswith('data:'):
        return False
        
    return True


def resolve_link(link: str, current_file: Path, deploy_dir: Path) -> Path:
    """Resolve a link to an absolute path."""
    parsed = urlparse(link)
    path = unquote(parsed.path)
    
    # Handle azul.rs links - treat them as relative to deploy root
    if parsed.netloc == 'azul.rs':
        path = path.lstrip('/')
        if not path:
            path = 'index.html'
        resolved = deploy_dir / path
    # Handle absolute paths (starting with /)
    elif path.startswith('/'):
        resolved = deploy_dir / path.lstrip('/')
    # Handle relative paths
    else:
        resolved = current_file.parent / path
        
    # Normalize the path
    resolved = resolved.resolve()
    
    # If it's a directory, look for index.html
    if resolved.is_dir():
        resolved = resolved / 'index.html'
    # If file doesn't exist but adding .html does, use that
    elif not resolved.exists() and not resolved.suffix:
        html_version = resolved.with_suffix('.html')
        if html_version.exists():
            resolved = html_version
        
    return resolved


def check_links(deploy_dir: Path) -> Tuple[int, int, List[Tuple[Path, str, str]]]:
    """
    Check all links in the deploy directory.
    
    Returns:
        Tuple of (total_links, broken_links, list of broken link details)
    """
    html_files = get_all_html_files(deploy_dir)
    
    total_links = 0
    broken_links = 0
    broken_details: List[Tuple[Path, str, str]] = []
    
    # Track all valid paths
    valid_paths: Set[Path] = set()
    for root, dirs, files in os.walk(deploy_dir):
        for file in files:
            valid_paths.add((Path(root) / file).resolve())
    
    print(f"Checking {len(html_files)} HTML files...")
    print(f"Deploy directory contains {len(valid_paths)} files total")
    print()
    
    for html_file in html_files:
        links = extract_links_from_file(html_file)
        file_broken = []
        
        for attr_name, url in links:
            if not is_internal_link(url):
                continue
                
            total_links += 1
            
            try:
                resolved = resolve_link(url, html_file, deploy_dir)
                
                # Check if the file exists
                if not resolved.exists():
                    # Also check without index.html for directory-style URLs
                    if resolved.name == 'index.html':
                        alt_path = resolved.parent
                        if not alt_path.exists():
                            file_broken.append((url, str(resolved)))
                    else:
                        file_broken.append((url, str(resolved)))
            except Exception as e:
                file_broken.append((url, f"Error: {e}"))
        
        if file_broken:
            rel_path = html_file.relative_to(deploy_dir)
            for url, resolved in file_broken:
                broken_links += 1
                broken_details.append((rel_path, url, resolved))
    
    return total_links, broken_links, broken_details


def main():
    # Determine deploy directory
    if len(sys.argv) > 1:
        deploy_dir = Path(sys.argv[1])
    else:
        script_dir = Path(__file__).parent
        deploy_dir = script_dir.parent / 'target' / 'deploy'
    
    deploy_dir = deploy_dir.resolve()
    
    if not deploy_dir.exists():
        print(f"Error: Deploy directory does not exist: {deploy_dir}")
        sys.exit(1)
    
    print(f"Checking links in: {deploy_dir}")
    print("=" * 60)
    
    total, broken, details = check_links(deploy_dir)
    
    print("=" * 60)
    print(f"Results: {total} internal links checked")
    print()
    
    if broken == 0:
        print("✅ All links are valid!")
        sys.exit(0)
    else:
        print(f"❌ Found {broken} broken links:")
        print()
        
        # Group by file
        by_file: Dict[Path, List[Tuple[str, str]]] = {}
        for file, url, resolved in details:
            if file not in by_file:
                by_file[file] = []
            by_file[file].append((url, resolved))
        
        for file, links in sorted(by_file.items()):
            print(f"  {file}:")
            for url, resolved in links:
                print(f"    - {url}")
                print(f"      → {resolved}")
            print()
        
        sys.exit(1)


if __name__ == '__main__':
    main()
