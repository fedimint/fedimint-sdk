#!/usr/bin/env python3
"""
Fedimint SDK setup script.
Downloads and patches all required SDK files into static/sdk/.
Works on Windows, macOS, and Linux — requires only Python 3.9+.
"""

import urllib.request
import os
import re

SDK_DIR = os.path.join(os.path.dirname(__file__), "static", "sdk")

FILES = [
    ("https://esm.sh/@fedimint/core@0.1.3/es2022/core.mjs",                                          "core.mjs"),
    ("https://esm.sh/@fedimint/core@0.1.3/es2022/testing.mjs",                                       "testing.mjs"),
    ("https://esm.sh/@fedimint/transport-web@0.1.2/es2022/transport-web.mjs",                        "transport-web.mjs"),
    ("https://esm.sh/@fedimint/types@0.0.3/es2022/types.mjs",                                        "types.mjs"),
    ("https://unpkg.com/@fedimint/transport-web@0.1.2/dist/worker.js",                               "worker.js"),
    ("https://unpkg.com/@fedimint/fedimint-client-wasm-bundler@0.1.1/fedimint_client_wasm_bg.js",    "fedimint_client_wasm_bg.js"),
    ("https://unpkg.com/@fedimint/fedimint-client-wasm-bundler@0.1.1/fedimint_client_wasm_bg.wasm",  "fedimint_client_wasm_bg.wasm"),
]

WASM_LOADER = """\
import * as bgModule from "/static/sdk/fedimint_client_wasm_bg.js";
export * from "/static/sdk/fedimint_client_wasm_bg.js";

const response = await fetch("/static/sdk/fedimint_client_wasm_bg.wasm");
const bytes = await response.arrayBuffer();
const { instance } = await WebAssembly.instantiate(bytes, {
    "./fedimint_client_wasm_bg.js": bgModule
});
const wasm = instance.exports;
bgModule.__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
"""

TEXT_EXTENSIONS = (".mjs", ".js")

REPLACEMENTS = [
    (r"https://esm\.sh/@fedimint/core@0\.1\.3/es2022/",          "/static/sdk/"),
    (r"https://esm\.sh/@fedimint/transport-web@0\.1\.2/es2022/", "/static/sdk/"),
    (r"https://esm\.sh/@fedimint/types@0\.0\.3/es2022/",         "/static/sdk/"),
    (r'from"\./([^"]+)"',                                         r'from"/static/sdk/\1"'),
    (r'"@fedimint/fedimint-client-wasm-bundler"',                 '"/static/sdk/fedimint_client_wasm.js"'),
    (r'new URL\("\.\/worker\.js",import\.meta\.url\)',            'new URL("/static/sdk/worker.js", location.origin)'),
    (r'from"/@fedimint/types@0\.0\.3/es2022/types\.mjs"',        'from"/static/sdk/types.mjs"'),
]


def download(url, dest):
    print(f"  Downloading {os.path.basename(dest)}...")
    urllib.request.urlretrieve(url, dest)


def patch(path):
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    for pattern, replacement in REPLACEMENTS:
        content = re.sub(pattern, replacement, content)
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)


def main():
    os.makedirs(SDK_DIR, exist_ok=True)
    print("Downloading Fedimint SDK files...")

    for url, filename in FILES:
        download(url, os.path.join(SDK_DIR, filename))

    print("Patching imports...")
    for _, filename in FILES:
        if filename.endswith(TEXT_EXTENSIONS):
            patch(os.path.join(SDK_DIR, filename))

    print("Writing wasm loader...")
    with open(os.path.join(SDK_DIR, "fedimint_client_wasm.js"), "w", encoding="utf-8") as f:
        f.write(WASM_LOADER)

    print("Done. SDK ready in static/sdk/")


if __name__ == "__main__":
    main()