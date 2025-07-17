#!/bin/sh

vendor=${vendor:-"fujitsu"}
device=$(lsusb | grep -i $vendor | awk '{print $6}')

if [ -z "$device" ]; then
    echo "$vendor scanner not found."
    exit 1
fi

echo "$vendor scanner found: $device"

# Extract the VID and PID
vid=$(echo $device | cut -d':' -f1)
pid=$(echo $device | cut -d':' -f2)

# Found device string
found_device="usb 0x${vid} 0x${pid}"

#Add the scanner VID:PID
echo $found_device > "/etc/sane.d/$vendor.conf"
echo $found_device > "/etc/scanbd/$vendor.conf"
