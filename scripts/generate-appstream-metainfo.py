import json
import xml.etree.ElementTree as ET
from xml.dom import minidom
from pathlib import Path

repo = Path('.')
captions_file = repo / 'artifacts/store-2026-11-13/captions.json'
captions = json.load(open(captions_file, encoding='utf-8'))

# AppStream language codes
lang_map = {
    'en-US': 'en',
    'es-419': 'es',
    'de': 'de',
    'fr': 'fr',
    'it': 'it',
    'ja': 'ja',
    'ko': 'ko',
    'nl': 'nl',
    'pl': 'pl',
    'pt-BR': 'pt_BR',
    'ru': 'ru',
    'sv': 'sv',
    'tr': 'tr',
    'uk': 'uk',
    'vi': 'vi',
    'zh-Hans': 'zh_CN',
    'zh-Hant': 'zh_TW'
}

screenshot_files = [
    ("atlas-store-01-split.png", True),
    ("atlas-store-02-search.png", False),
    ("atlas-store-03-preview-img.png", False),
    ("atlas-store-04-convert.png", False),
    ("atlas-store-05-palette.png", False),
    ("atlas-store-06-tree.png", False),
    ("atlas-store-07-archive.png", False),
    ("atlas-store-08-cloud.png", False),
    ("atlas-store-09-theme.png", False),
    ("atlas-store-10-settings.png", False),
]

summary_map = {
    'en': 'Fast, GPU-accelerated modern file manager',
    'es': 'Administrador de archivos moderno y acelerado por GPU',
    'de': 'Schneller, GPU-beschleunigter moderner Dateimanager',
    'fr': 'Gestionnaire de fichiers moderne et rapide accéléré par GPU',
    'it': 'Gestore di file moderno e veloce con accelerazione GPU',
    'ja': 'GPU アクセラレーション対応の高速でモダンなファイルマネージャー',
    'ko': 'GPU 가속을 지원하는 빠르고 현대적인 파일 관리자',
    'nl': 'Snelle, GPU-versnelde moderne bestandsbeheerder',
    'pl': 'Szybki, nowoczesny menedżer plików z akceleracją GPU',
    'pt_BR': 'Gerenciador de arquivos moderno e rápido com aceleração por GPU',
    'ru': 'Быстрый современный файловый менеджер с аппаратным ускорением GPU',
    'sv': 'Snabb, GPU-accelererad modern filhanterare',
    'tr': 'GPU hızlandırmalı hızlı ve modern dosya yöneticisi',
    'uk': 'Швидкий сучасний файловий менеджер із прискоренням GPU',
    'vi': 'Trình quản lý tệp hiện đại, nhanh chóng với tăng tốc GPU',
    'zh_CN': '支持 GPU 加速的高性能现代化文件管理器',
    'zh_TW': '支援 GPU 加速的高效能現代化檔案管理員'
}

desc_p1 = {
    'en': 'Atlas is a high-performance modern file manager built with Rust and GPU acceleration. It features tabs, split panes, instant search, in-line previews, and quick file conversions.',
    'es': 'Atlas es un administrador de archivos moderno y de alto rendimiento creado con Rust y aceleración por GPU. Cuenta con pestañas, paneles divididos, búsqueda instantánea, vistas previas y conversiones rápidas.',
    'de': 'Atlas ist ein moderner Hochleistungs-Dateimanager mit Rust und GPU-Beschleunigung. Er bietet Tabs, geteilte Ansichten, Sofortsuche, integrierte Dateivorschau und schnelle Dateikonvertierung.',
    'fr': 'Atlas est un gestionnaire de fichiers moderne et performant conçu en Rust avec accélération GPU. Il propose des onglets, des volets côte à côte, une recherche instantanée et des aperçus de fichiers.',
    'zh_CN': 'Atlas 是一款采用 Rust 开发并支持 GPU 加速的高性能现代化文件管理器。它提供标签页、分栏视图、即时子目录搜索、多格式即时预览和快速文件转换功能。'
}

root = ET.Element('component', {'type': 'desktop-application'})
ET.SubElement(root, 'id').text = 'com.atlasnative.Atlas'
ET.SubElement(root, 'metadata_license').text = 'CC0-1.0'
ET.SubElement(root, 'project_license').text = 'AGPL-3.0-or-later'
ET.SubElement(root, 'name').text = 'Atlas'

for lang, summary in summary_map.items():
    if lang == 'en':
        ET.SubElement(root, 'summary').text = summary
    else:
        ET.SubElement(root, 'summary', {'xml:lang': lang}).text = summary

desc_el = ET.SubElement(root, 'description')
for lang, p_text in desc_p1.items():
    if lang == 'en':
        ET.SubElement(desc_el, 'p').text = p_text
    else:
        ET.SubElement(desc_el, 'p', {'xml:lang': lang}).text = p_text

ul_el = ET.SubElement(desc_el, 'ul')
features = [
    'Tabs and split panes, side by side, each with independent views and history',
    'Instant search across a folder and all of its subfolders',
    'In-line previews for images, PDFs, Markdown, text, audio and code',
    'Format conversion ring for images, audio and document formats',
    'Open ZIP archives like normal folders with instant preview',
    'Remote and cloud integrations: Google Drive, OneDrive, SFTP, FTP, WebDAV and SMB',
    'Complete command palette (Ctrl+Shift+P) and customizable keyboard shortcuts'
]
for f in features:
    ET.SubElement(ul_el, 'li').text = f

ET.SubElement(root, 'launchable', {'type': 'desktop-id'}).text = 'atlas.desktop'
ET.SubElement(root, 'url', {'type': 'homepage'}).text = 'https://atlasnative.com'
ET.SubElement(root, 'url', {'type': 'bugtracker'}).text = 'https://github.com/neondeex/atlas-filemanager/issues'
ET.SubElement(root, 'url', {'type': 'vcs-browser'}).text = 'https://github.com/neondeex/atlas-filemanager'
dev_el = ET.SubElement(root, 'developer', {'id': 'io.github.neondeex'})
ET.SubElement(dev_el, 'name').text = 'NeonDeex'
ET.SubElement(root, 'project_group').text = 'Atlas'

screenshots_el = ET.SubElement(root, 'screenshots')
base_url = "https://raw.githubusercontent.com/neondeex/atlas-filemanager/v0.1.6-beta.4/docs/images/screenshots/"

for idx, (filename, is_default) in enumerate(screenshot_files):
    attrs = {'type': 'default'} if is_default else {}
    s_el = ET.SubElement(screenshots_el, 'screenshot', attrs)
    ET.SubElement(s_el, 'image', {'type': 'source'}).text = base_url + filename
    
    # Add caption in all available languages
    for raw_lang, lang_code in lang_map.items():
        if raw_lang in captions and idx < len(captions[raw_lang]):
            cap_text = captions[raw_lang][idx]
            if lang_code == 'en':
                ET.SubElement(s_el, 'caption').text = cap_text
            else:
                ET.SubElement(s_el, 'caption', {'xml:lang': lang_code}).text = cap_text

releases_el = ET.SubElement(root, 'releases')
rel_el4 = ET.SubElement(releases_el, 'release', {'version': '0.1.6-beta.4', 'date': '2026-11-21'})
ET.SubElement(rel_el4, 'url').text = 'https://github.com/neondeex/atlas-filemanager/releases/tag/v0.1.6-beta.4'
rel_desc4 = ET.SubElement(rel_el4, 'description')
ET.SubElement(rel_desc4, 'p').text = 'File dates now show in local time, improved Linux Wayland clipboard support, multi-disk listing deduplication, and setting to make Atlas default file manager.'

rel_el = ET.SubElement(releases_el, 'release', {'version': '0.1.6-beta.2', 'date': '2026-11-13'})
ET.SubElement(rel_el, 'url').text = 'https://github.com/neondeex/atlas-filemanager/releases/tag/v0.1.6-beta.2'
rel_desc = ET.SubElement(rel_el, 'description')
ET.SubElement(rel_desc, 'p').text = 'Atlas 0.1.6-beta.2 introduces session save on window close, improved drag and drop for unselected files, sort commands in the command palette, and address bar preview toggles.'

ET.SubElement(root, 'content_rating', {'type': 'oars-1.1'})
provides_el = ET.SubElement(root, 'provides')
ET.SubElement(provides_el, 'binary').text = 'atlas'

xml_str = ET.tostring(root, encoding='utf-8')
parsed = minidom.parseString(xml_str)
pretty_xml = parsed.toprettyxml(indent='  ', encoding='utf-8')

out_path = repo / 'packaging/linux/com.atlasnative.Atlas.metainfo.xml'
with open(out_path, 'wb') as f:
    f.write(pretty_xml)
print(f"Generated {out_path} ({len(pretty_xml)} bytes)")

# Also output the canonical GitHub-namespaced ID required for zero-friction Flathub submission
root_io = ET.fromstring(xml_str)
root_io.find('id').text = 'io.github.neondeex.atlas-filemanager'
root_io.find('launchable').text = 'io.github.neondeex.atlas-filemanager.desktop'
parsed_io = minidom.parseString(ET.tostring(root_io, encoding='utf-8'))
pretty_io = parsed_io.toprettyxml(indent='  ', encoding='utf-8')
out_io = repo / 'packaging/linux/io.github.neondeex.atlas-filemanager.metainfo.xml'
with open(out_io, 'wb') as f:
    f.write(pretty_io)
print(f"Generated {out_io} ({len(pretty_io)} bytes)")
