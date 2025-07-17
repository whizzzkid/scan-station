#!/bin/sh

now=`date +"%Y-%m-%d-%H%M%S"`
vendor=${vendor:-"fujitsu"}
dpi=${dpi:-300}
mode=${mode:-"Color"}

# Run the scanning command
/app/sane-scan-pdf/scan -d -x $vendor -r $dpi -v --mode $mode --crop -o /scans/scan-$now.pdf
