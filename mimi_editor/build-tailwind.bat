@echo off
setlocal

rem Windows equivalent of build-tailwind.sh, for shells without bash.
rem Both scripts cache the same .tailwindcss.exe next to this file.

set "project_dir=%~dp0"
set "tailwind_bin=%project_dir%.tailwindcss.exe"
set "input_css=%project_dir%extensions\MimiIncubator\resources\tailwind.input.css"
set "output_css=%project_dir%extensions\MimiIncubator\resources\tailwind.css"
set "version=v3.4.17"

if /i "%PROCESSOR_ARCHITECTURE%"=="ARM64" (
	set "asset=tailwindcss-windows-arm64.exe"
) else (
	set "asset=tailwindcss-windows-x64.exe"
)

if not exist "%tailwind_bin%" (
	echo Downloading Tailwind CSS CLI %version%...
	curl --fail --location --silent --show-error ^
		"https://github.com/tailwindlabs/tailwindcss/releases/download/%version%/%asset%" ^
		--output "%tailwind_bin%"
	if errorlevel 1 (
		echo Failed to download %asset%. 1>&2
		exit /b 1
	)
)

"%tailwind_bin%" --config "%project_dir%tailwind.config.js" ^
	--input "%input_css%" --output "%output_css%" --minify
if errorlevel 1 exit /b 1

echo Built extensions\MimiIncubator\resources\tailwind.css
