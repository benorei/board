# board

A real-time transit departure board built with [Slint](https://slint.dev/), displaying arrivals from the [OneBusAway](https://onebusaway.org/) Puget Sound API.

This is a fork of the original project by [haylinmoore](https://github.com/haylinmoore/board).

Designed to run on a PROXmobil3 with a framebuffer display, but also works as a desktop window for development.

Displays route icons for Link, Sounder, the Seattle Center Monorail, Metro RapidRide, and Swift BRT.

![The transit board showing departures for Westlake Station](readme-photos/example2.png)
![The transit board showing departures for the Hwy 99 & 196 St bus stop](readme-photos/example3.png)

## Configuration

Requires the `OBA_API_KEY` environment variable to be set with an [OneBusAway Puget Sound API key](https://www.soundtransit.org/help-contacts/business-information/open-transit-data-otd). Email `oba_api_key@soundtransit.org` to request one. Until then, you can use the key `TEST` which doesn't use real data but will still show something!

## Usage
Takes no command line arguments.

```
board
```

Example:
```
OBA_API_KEY=your-key cargo run
```

## How to use / QR Codes
After startup, it will begin to read from the barcode scanner (/dev/ttyUSB0) on the proxmobil3 or the command line (stdin) on desktop. QR codes or command line inputs should be formatted like:

```
Station Name:station_code_1 station_code_2 ...\n
```
The newline at the end is required because the code uses the `getline` function.
The barcode scanner can be configured to automatically send a newline after every scan, or you can put one manually in the qr code.

The station/stop codes must be prefixed with the agency number (40 for Sound Transit and 1 for King County Metro). If you're using an app like OneBusAway to get the stop number, prefix the agency number to what you see in the app. For example, the Broadway E & E Mercer St northbound stop is listed as #11200 in the app, but you should enter 1_11200 in this program.

The best way to make a QR code is the command line utility `qrencode` (`brew install qrencode` on mac). Don't use online QR code generators, as these often put their own links in the QR code that redirect to your content, which will not work on the pm3.

Scanning a new qr code will let you change the station at any time!

## Example QR codes
<img src="qrs/udistrict-withbus.jpeg" width=200></img>
<img src="qrs/udistrict-nobus.jpeg" width=200></img>
<img src="qrs/evergreen-pt.jpeg" width=200></img>
## Building

### Desktop (x86, windowed) - untested outside of macos

```
echo "U District:40_990001 40_990002" | OBA_API_KEY=your-key cargo run
```

### ARM (PROXmobil3, framebuffer)

```
cargo zigbuild --release --target armv7-unknown-linux-gnueabihf.2.28 --no-default-features --features framebuffer
```

## Deploying to PROXmobil3

1. Build the ARM binary (see above).

2. Copy the binary to the device over SSH:
   ```
   cat target/armv7-unknown-linux-gnueabihf/release/board | ssh root@<DEVICE_IP> 'cat > /init/board/exe && chmod +x /init/board/exe'
   ```

3. Create the systemd service at `/init/board/board.service` on the device:
   ```ini
   [Unit]
   Description=Board Display Service
   After=network.target

   [Service]
   Type=simple
   Environment=OBA_API_KEY=your-key
   ExecStart=/init/board/exe
   Restart=always
   RestartSec=2
   WorkingDirectory=/init/board

   [Install]
   WantedBy=multi-user.target
   ```

4. Create the autorun script at `/init/autorun/99-board.sh` to install the service on boot:
   ```sh
   #!/bin/sh

   mount -o remount,rw /

   # Set timezone (adjust as needed)
   ln -sf /usr/share/zoneinfo/America/Los_Angeles /etc/localtime

   # Disable default PM3 UI
   systemctl stop nx
   systemctl mask nx
   systemctl stop init-abtproxy
   systemctl mask init-abtproxy

   # Install and start board service
   cp /init/board/board.service /etc/systemd/system/
   systemctl daemon-reload
   systemctl enable board.service

   mount -o remount,ro /

   /usr/bin/NxExe watchdog 0

   systemctl start board.service
   ```

5. To update the stop or API key later:
   ```
   ssh root@<DEVICE_IP>
   mount -o remount,rw /
   vi /init/board/board.service
   mount -o remount,ro /
   systemctl daemon-reload
   systemctl restart board.service
   ```

## Debug CLI

Inspect raw API data and see how arrivals are deduplicated:

```
OBA_API_KEY=your-key cargo run --bin debug_arrivals -- "University District" 40_990001 40_990002
```

## Development

Uses Nix for the development environment:

```
nix-shell
```

## License

[MIT](LICENSE)

"Trip Not Real-time" icon from: https://www.svgrepo.com/svg/451785/network-wireless-no-route
Ferry icon from: https://www.svgrepo.com/svg/15727/ferry
Washington State DOT logo from: https://commons.wikimedia.org/wiki/File:WSDOT_Logo.svg
Sound Transit route linebullets from: https://commons.wikimedia.org/wiki/File:Line_1_(Sound_Transit)_icon.svg
