#!/bin/sh

date_format="${date_format:-"%Y-%m-%d-%H%M%S-%3N-%Z"}"
now=`date +"$date_format"`
file_prefix="${file_prefix:-"scan"}"
vendor=${vendor:-"fujitsu"}
dpi=${dpi:-300}
mode=${mode:-"Color"}

# Run the scanning command and save the output to a temporary file
/app/sane-scan-pdf/scan -v -d -x $vendor -r $dpi --mode $mode --skip-empty-pages -o /tmp/$file_prefix-$now.pdf

# Once the scan is complete, move the file to the output directory
mv /tmp/$file_prefix-$now.pdf /scans/$file_prefix-$now.pdf

# This is needed to ensure the file is ready to be imported by the next step.
