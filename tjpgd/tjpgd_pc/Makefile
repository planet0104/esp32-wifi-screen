# Makefile for TJpgDec Windows PC version

CC = gcc
CFLAGS = -O2 -Wall
TARGET = tjpgd_test.exe
SOURCES = main.c tjpgd.c
OBJECTS = $(SOURCES:.c=.o)

.PHONY: all clean test

all: $(TARGET)

$(TARGET): $(OBJECTS)
	$(CC) $(CFLAGS) -o $@ $^

%.o: %.c
	$(CC) $(CFLAGS) -c $<

clean:
	del /Q $(OBJECTS) $(TARGET) 2>nul || echo.

test: $(TARGET)
	@if exist test.jpg (
		$(TARGET) test.jpg test_output.ppm
	) else (
		echo Error: test.jpg not found
		echo Please provide a test JPEG image
	)
