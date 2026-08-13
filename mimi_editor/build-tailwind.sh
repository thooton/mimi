#!/usr/bin/env bash
set -euo pipefail

project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
input_css="$project_dir/extensions/MimiIncubator/resources/tailwind.input.css"
output_css="$project_dir/extensions/MimiIncubator/resources/tailwind.css"
version="v3.4.17"

case "$(uname -s)-$(uname -m)" in
	Linux-x86_64) asset="tailwindcss-linux-x64" ;;
	Linux-aarch64|Linux-arm64) asset="tailwindcss-linux-arm64" ;;
	Darwin-x86_64) asset="tailwindcss-macos-x64" ;;
	Darwin-arm64) asset="tailwindcss-macos-arm64" ;;
	# Git Bash and MSYS2 report MINGW64_NT-*, MSYS_NT-* or CYGWIN_NT-*.
	MINGW*-x86_64|MSYS*-x86_64|CYGWIN*-x86_64) asset="tailwindcss-windows-x64.exe" ;;
	MINGW*-aarch64|MSYS*-aarch64|CYGWIN*-aarch64) asset="tailwindcss-windows-arm64.exe" ;;
	*) echo "Unsupported platform: $(uname -s) $(uname -m)" >&2; exit 1 ;;
esac

# Share the cached binary with build-tailwind.bat, which needs the .exe suffix.
tailwind_bin="$project_dir/.tailwindcss"
case "$asset" in
	*.exe) tailwind_bin="$tailwind_bin.exe" ;;
esac

if [[ ! -x "$tailwind_bin" ]]; then
	echo "Downloading Tailwind CSS CLI $version…"
	curl --fail --location --silent --show-error \
		"https://github.com/tailwindlabs/tailwindcss/releases/download/$version/$asset" \
		--output "$tailwind_bin"
	chmod +x "$tailwind_bin"
fi

"$tailwind_bin" --config "$project_dir/tailwind.config.js" \
	--input "$input_css" --output "$output_css" --minify
echo "Built ${output_css#"$project_dir/"}"
