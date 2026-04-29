#!/bin/bash
set -e

cross build --release --target armv7-unknown-linux-musleabihf
cp target/armv7-unknown-linux-musleabihf/release/weather-kindle kual/bin/weather
chmod +x kual/bin/weather

cp -R kual/* /Volumes/Kindle/extensions/infonaytto/
cp config.toml /Volumes/Kindle/extensions/infonaytto/
