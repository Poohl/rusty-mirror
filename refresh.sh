firefox --headless --window-size 800,480 -P default --screenshot "http://localhost:8070/"

convert screenshot.png -remap colors.png +dither -rotate 180 calender_wbr.png

convert calender_wbr.png -fill white -fuzz 10% +opaque black -depth 1 -type Bilevel BMP3:black.bmp

convert calender_wbr.png -fill white -fuzz 30% +opaque red -fill black -opaque red BMP3:red.bmp

if diff black.bmp black_old.bmp && diff red.bmp red_old.bmp;
then
	echo ""
else
	echo "changed"
	python3 e-Paper/RaspberryPi_JetsonNano/python/examples/epd_7in5b_V2_test.py
	cp black.bmp black_old.bmp
	cp red.bmp red_old.bmp
fi
