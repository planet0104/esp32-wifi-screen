/*----------------------------------------------------------------------------/
/ JPEG to BMP Converter using TJpgDec
/ This program converts JPEG files to BMP format on Windows PC
/----------------------------------------------------------------------------*/

#define _CRT_SECURE_NO_WARNINGS
#define DEBUG_LOG 1

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stdint.h>
#include "tjpgd.h"

#pragma pack(push, 1)
/* BMP file header structure */
typedef struct {
    uint16_t bfType;        /* File type, must be 'BM' */
    uint32_t bfSize;        /* File size in bytes */
    uint16_t bfReserved1;   /* Reserved, must be 0 */
    uint16_t bfReserved2;   /* Reserved, must be 0 */
    uint32_t bfOffBits;     /* Offset to bitmap data */
} BITMAPFILEHEADER;

/* BMP info header structure */
typedef struct {
    uint32_t biSize;           /* Size of this header */
    int32_t  biWidth;          /* Image width */
    int32_t  biHeight;         /* Image height */
    uint16_t biPlanes;         /* Number of color planes */
    uint16_t biBitCount;       /* Bits per pixel */
    uint32_t biCompression;    /* Compression type */
    uint32_t biSizeImage;      /* Image size in bytes */
    int32_t  biXPelsPerMeter;  /* Horizontal resolution */
    int32_t  biYPelsPerMeter;  /* Vertical resolution */
    uint32_t biClrUsed;        /* Number of colors used */
    uint32_t biClrImportant;   /* Number of important colors */
} BITMAPINFOHEADER;
#pragma pack(pop)

/* User defined device identifier */
typedef struct {
    FILE *fp;               /* File pointer */
    uint8_t *fbuf;          /* Pointer to the frame buffer */
    unsigned int wfbuf;     /* Width of the frame buffer [pix] */
} IODEV;


/* User defined input function */
size_t in_func (JDEC* jd, uint8_t* buff, size_t nbyte)
{
    IODEV *dev = (IODEV*)jd->device;
    if (buff) {
        return fread(buff, 1, nbyte, dev->fp);
    } else {
        return fseek(dev->fp, nbyte, SEEK_CUR) ? 0 : nbyte;
    }
}


/* User defined output function */
int out_func (JDEC* jd, void* bitmap, JRECT* rect)
{
    IODEV *dev = (IODEV*)jd->device;
    uint8_t *src, *dst;
    uint16_t y, bws, bwd;

    src = (uint8_t*)bitmap;
    dst = dev->fbuf + 3 * (rect->top * dev->wfbuf + rect->left);
    bws = 3 * (rect->right - rect->left + 1);
    bwd = 3 * dev->wfbuf;
    for (y = rect->top; y <= rect->bottom; y++) {
        memcpy(dst, src, bws);
        src += bws;
        dst += bwd;
    }
    return 1;
}


/* Save the frame buffer to BMP file */
int save_bmp (const char *filename, uint8_t *framebuffer, int width, int height)
{
    FILE *fp;
    BITMAPFILEHEADER bfh;
    BITMAPINFOHEADER bih;
    int row_size, padding, y;
    uint8_t *row_buffer;
    uint8_t pad_bytes[3] = {0, 0, 0};
    
    fp = fopen(filename, "wb");
    if (!fp) {
        printf("Error: Cannot create output file %s\n", filename);
        return 0;
    }
    
    row_size = width * 3;
    padding = (4 - (row_size % 4)) % 4;
    
    bfh.bfType = 0x4D42;
    bfh.bfSize = sizeof(BITMAPFILEHEADER) + sizeof(BITMAPINFOHEADER) + (row_size + padding) * height;
    bfh.bfReserved1 = 0;
    bfh.bfReserved2 = 0;
    bfh.bfOffBits = sizeof(BITMAPFILEHEADER) + sizeof(BITMAPINFOHEADER);
    
    bih.biSize = sizeof(BITMAPINFOHEADER);
    bih.biWidth = width;
    bih.biHeight = height;
    bih.biPlanes = 1;
    bih.biBitCount = 24;
    bih.biCompression = 0;
    bih.biSizeImage = (row_size + padding) * height;
    bih.biXPelsPerMeter = 2835;
    bih.biYPelsPerMeter = 2835;
    bih.biClrUsed = 0;
    bih.biClrImportant = 0;
    
    fwrite(&bfh, sizeof(BITMAPFILEHEADER), 1, fp);
    fwrite(&bih, sizeof(BITMAPINFOHEADER), 1, fp);
    
    row_buffer = (uint8_t*)malloc(row_size);
    if (!row_buffer) {
        fclose(fp);
        printf("Error: Cannot allocate row buffer\n");
        return 0;
    }
    
    for (y = height - 1; y >= 0; y--) {
        uint8_t *src = framebuffer + y * row_size;
        int x;
        for (x = 0; x < width; x++) {
            row_buffer[x * 3 + 0] = src[x * 3 + 2];
            row_buffer[x * 3 + 1] = src[x * 3 + 1];
            row_buffer[x * 3 + 2] = src[x * 3 + 0];
        }
        fwrite(row_buffer, 1, row_size, fp);
        if (padding > 0) {
            fwrite(pad_bytes, 1, padding, fp);
        }
    }
    
    free(row_buffer);
    fclose(fp);
    printf("Output saved to %s\n", filename);
    return 1;
}


/* Generate output filename */
char* generate_output_filename(const char *input_file)
{
    static char output_file[512];
    const char *dot;
    size_t len;
    
    dot = strrchr(input_file, '.');
    if (dot && (dot - input_file) < sizeof(output_file) - 5) {
        len = dot - input_file;
        memcpy(output_file, input_file, len);
        strcpy(output_file + len, ".bmp");
    } else {
        snprintf(output_file, sizeof(output_file), "%s.bmp", input_file);
    }
    return output_file;
}


/* Main function */
int main(int argc, char *argv[])
{
    JRESULT res;
    JDEC jdec;
    void *work;
    IODEV devid;
    size_t sz_work = TJPGD_WORKSPACE_SIZE;
    uint8_t *framebuffer;
    const char *input_file;
    const char *output_file;
    
    printf("JPEG to BMP Converter using TJpgDec\n");
    printf("====================================\n\n");
    
    if (argc < 2) {
        printf("Usage: %s <input.jpg> [output.bmp]\n", argv[0]);
        printf("  input.jpg  - Input JPEG file\n");
        printf("  output.bmp - Output BMP file (optional, auto-generated if not specified)\n");
        printf("\nExamples:\n");
        printf("  %s monitor.jpg              -> monitor.bmp\n", argv[0]);
        printf("  %s photo.jpg output.bmp     -> output.bmp\n", argv[0]);
        return 1;
    }
    
    input_file = argv[1];
    output_file = (argc >= 3) ? argv[2] : generate_output_filename(input_file);
    
    devid.fp = fopen(input_file, "rb");
    if (!devid.fp) {
        printf("Error: Cannot open input file %s\n", input_file);
        return 1;
    }
    printf("Input file: %s\n", input_file);
    printf("Output file: %s\n", output_file);
    
    work = malloc(sz_work);
    if (!work) {
        printf("Error: Cannot allocate work area (%u bytes)\n", (unsigned int)sz_work);
        fclose(devid.fp);
        return 1;
    }
    
    res = jd_prepare(&jdec, in_func, work, sz_work, &devid);
    if (res != JDR_OK) {
        printf("Error: jd_prepare() failed (rc=%d)\n", res);
        free(work);
        fclose(devid.fp);
        return 1;
    }
    
    printf("Image size: %u x %u\n", jdec.width, jdec.height);
    printf("Components: %u\n", jdec.ncomp);
    printf("MCU size: %u x %u blocks\n", jdec.msx, jdec.msy);
    
    framebuffer = malloc(jdec.width * jdec.height * 3);
    if (!framebuffer) {
        printf("Error: Cannot allocate frame buffer\n");
        free(work);
        fclose(devid.fp);
        return 1;
    }
    
    devid.fbuf = framebuffer;
    devid.wfbuf = jdec.width;
    
    printf("Decompressing...\n");
    res = jd_decomp(&jdec, out_func, 0);
    if (res != JDR_OK) {
        printf("Error: jd_decomp() failed (rc=%d)\n", res);
        free(framebuffer);
        free(work);
        fclose(devid.fp);
        return 1;
    }
    
    printf("Decompression completed successfully!\n");
    
    save_bmp(output_file, framebuffer, jdec.width, jdec.height);
    
    free(framebuffer);
    free(work);
    fclose(devid.fp);
    
    printf("\nDone!\n");
    return 0;
}








