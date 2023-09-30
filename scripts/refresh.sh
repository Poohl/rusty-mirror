#!/bin/sh

# Take a screenshot of the website in the correct size
firefox --headless --window-size 800,480 -P default --screenshot "http://localhost:8070/"

# firefox screenshot is buggy and only works without destination...
mv screenshot.png ../cache/screenshot.png

# reduce it down to the tree colors the eink display supports (and flip it upside down)
convert ../cache/screenshot.png -remap colors.png +dither -rotate 180 ../cache/calender_wbr.png

# extract red and black
convert ../cache/calender_wbr.png -fill white -fuzz 10% +opaque black -depth 1 -type Bilevel BMP3:../cache/black.bmp

convert ../cache/calender_wbr.png -fill white -fuzz 30% +opaque red -fill black -opaque red BMP3:../cache/red.bmp

# figure out if something changed, it takes ages to refresh that screen
if diff ../cache/black.bmp ../cache/black_old.bmp && diff ../cache/red.bmp ../cache/red_old.bmp;
then
	echo "no change"
else
	echo "changed"
	#python3 e-Paper/RaspberryPi_JetsonNano/python/examples/epd_7in5b_V2_test.py
	cp ../cache/black.bmp ../cache/black_old.bmp
	cp ../cache/red.bmp ../cache/red_old.bmp
fi