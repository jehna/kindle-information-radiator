# Kindle information radiator
> A super simple info display app for Kindle

I tried to use TRMNL client for Kindle, but since Kindle has a full blown Linux in it, why not to use the full power of the OS to display information on an E-ink dsplay?

## Installing

After connecting your jailbroken Kindle via USB data cable to MacOS, run:

```shell
./build.sh
```

This builds and installs the application as KUAL extension

### Initial Configuration

This project needs some initial configuration in order to work. Copy the [config.example.toml](./config.example.toml) to `config.toml` before building the app for the first time and fill in necessary values.

## Features

A minimal info display for Kindle with:
* Next 12h weather
* Next three iCal events
* MunApp integration for timing the taxi (don't ask)

## Contributing

This was intended as a solo project, but if you enjoy it, please make it more modular and shoot a pull request.

## Licensing

The code in this project is licensed under MIT license.
