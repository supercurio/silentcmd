#!/usr/bin/env gnuplot

plot \
    "/tmp/data.dat" using 1 title "RMS (dB)" with lines, \
    "/tmp/data.dat" using 2 title "Switch Status" with lines
pause -1
