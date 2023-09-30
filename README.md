# rusty Mirror

A simple rust app that turns iCal calendars into a Website.

use `cargo build` to compile. Requires the ssl system library

Usage:

```
Usage: my_mm [OPTIONS]

Options:
  -o, --output <OUTPUT>  [example: calendar.html]
  -c, --config <CONFIG>  [default: config.json]
  -s, --server <SERVER>  [example: 127.0.0.1:8080]
  -h, --help             Print help
```

for configuration check out the provided example.

The calendar refreshes at midnight once you place the first request.

# My Usecase

I use this in combination with firefox's html to png feature, some convert tricks and an e-ink display to view my calendar on my desk. See the scripts folder for how that works.
