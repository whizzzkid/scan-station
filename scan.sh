#!/bin/sh

date_format="${date_format:-"%Y-%m-%d-%H%M%S-%3N-%Z"}"
now=`date +"$date_format"`
file_prefix="${file_prefix:-"scan"}"
vendor=${vendor:-"fujitsu"}
dpi=${dpi:-300}
mode=${mode:-"Color"}

# Run the scanning command
/app/sane-scan-pdf/scan -d -x $vendor -r $dpi -v --mode $mode --crop -o /scans/$file_prefix-$now.pdf
