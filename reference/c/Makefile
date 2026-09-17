# C99 reference ScanChunk parser — CuNi exact-text wire (cuni#17 / Rider#22 SoT)
CC      ?= gcc
CFLAGS  ?= -std=c99 -Wall -Wextra -Wpedantic -O2
LDFLAGS ?=

.PHONY: all clean test

all: cuni_scan_cli

cuni_scan_cli: cuni_scan_cli.o cuni_scan_chunk.o
	$(CC) $(LDFLAGS) -o $@ $^

cuni_scan_cli.o: cuni_scan_cli.c cuni.h
	$(CC) $(CFLAGS) -c -o $@ cuni_scan_cli.c

cuni_scan_chunk.o: cuni_scan_chunk.c cuni.h
	$(CC) $(CFLAGS) -c -o $@ cuni_scan_chunk.c

test: cuni_scan_cli
	@./run_golden.sh

clean:
	rm -f cuni_scan_cli *.o
