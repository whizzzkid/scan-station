#!/bin/sh

date_format="${date_format:-"%Y-%m-%d-%H%M%S-%3N-%Z"}"
now=`date +"$date_format"`
file_prefix="${file_prefix:-"scan"}"
vendor=${vendor:-"fujitsu"}
dpi=${dpi:-300}
mode=${mode:-"Color"}
tmp_output_dir="/tmp/sane-scan-pdf-output"
file_name="${file_prefix}-${now}.pdf"

# Ensure the output directory exists
mkdir -p "$tmp_output_dir"
tmp_file="$tmp_output_dir/$file_name"
output_file="/scans/$file_name"

# Run the scanning command and save the output to a temporary file
/app/sane-scan-pdf/scan -v -d -x $vendor -r $dpi --mode $mode --skip-empty-pages --crop -o "$tmp_file"

if [ $? -ne 0 ]; then
    echo "Error: Scan command failed. Exiting."
    exit 1
fi

# Once the scan is complete, move the file to the output directory
echo "Scan completed successfully to $tmp_file. Moving file to $output_file"
mv "$tmp_file" "$output_file"

if [ $? -ne 0 ]; then
    echo "Error: Failed to move the file to $output_file. Please check permissions or directory existence." >&2
    exit 1
fi

# This is needed to ensure the file is ready to be imported by the next step.
