#!/bin/sh

date_format="${date_format:-"%Y-%m-%d-%H%M%S-%3N-%Z"}"
now=`date +"$date_format"`
file_prefix="${file_prefix:-"scan"}"
vendor=${vendor:-"fujitsu"}
dpi=${dpi:-300}
mode=${mode:-"Color"}

# Run the scanning command and save the output to a temporary file
/app/sane-scan-pdf/scan -v -d -x $vendor -r $dpi --mode $mode --skip-empty-pages -o /tmp/$file_prefix-$now.pdf

if [ $? -ne 0 ]; then
    echo "Error: Scan command failed. Exiting."
    exit 1
fi

# Once the scan is complete, move the file to the output directory
echo "Scan completed successfully. Moving file to /scans/$file_prefix-$now.pdf"
mv /tmp/$file_prefix-$now.pdf /scans/$file_prefix-$now.pdf

if [ $? -ne 0 ]; then
    echo "Error: Failed to move the file to /scans. Please check permissions or directory existence." >&2
    exit 1
fi

# This is needed to ensure the file is ready to be imported by the next step.
