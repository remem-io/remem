import json
import subprocess
import sys

ALLOWED = {
    "Apache-2.0",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "Unicode-3.0",
    "Unicode-DFS-2016",
    "Zlib",
    "OpenSSL",
    "BSL-1.0",
    "CC0-1.0",
    "MPL-2.0",
    "0BSD",
    "Unlicense"
}

def check():
    output = subprocess.check_output(["cargo", "metadata", "--format-version", "1"])
    data = json.loads(output)
    
    for pkg in data.get("packages", []):
        license_expr = pkg.get("license")
        if not license_expr:
            print(f"Warning: {pkg['name']} has no license")
            continue
            
        # Very simple check: see if any part of the expression is not in ALLOWED
        parts = license_expr.replace("(", "").replace(")", "").replace(" OR ", " ").replace(" AND ", " ").split()
        for p in parts:
            if p not in ALLOWED and p != "WITH" and p != "LLVM-exception" and p != "exception":
                print(f"Unallowed license {p} in {pkg['name']}: {license_expr}")

if __name__ == "__main__":
    check()
