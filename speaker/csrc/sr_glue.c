/*
 * What ESP-SR's prebuilt libraries expect from ESP-IDF, for a firmware
 * without it: the model list, logging and printf, FreeRTOS mutexes and
 * newlib locks. Memory allocation and the rest are in Rust (src/sr.rs).
 */
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include <sys/lock.h>

#include "esp_log.h"

/* Prints a finished line or fragment; implemented in Rust. */
void speaker_sr_print(int level, const char *tag, const char *text, size_t len);

/* ESP-SR's srmodel_data_t and srmodel_list_t (model_path.h) as compiled
 * into its libraries, that is with ESP_PLATFORM defined. */
typedef struct {
    int num;
    char **files;
    char **data;
    int *sizes;
} srmodel_data_t;

typedef struct {
    char **model_name;
    char **model_info;
    void *partition;
    void *mmap_handle;
    int num;
    srmodel_data_t **model_data;
} srmodel_list_t;

#define SRMODEL_STRING_LENGTH 32

static srmodel_list_t *static_srmodels;

static uint32_t read_int32(const char *data)
{
    const uint8_t *b = (const uint8_t *)data;
    return (uint32_t)b[0] | ((uint32_t)b[1] << 8) | ((uint32_t)b[2] << 16) | ((uint32_t)b[3] << 24);
}

/* Returns a _MODEL_INFO_ file's text from its first line that is not a
 * comment, without a final newline. */
static char *model_info(const char *data, int size)
{
    while (size > 0 && *data == '#') {
        while (size > 1 && *data != '\n') {
            data++;
            size--;
        }
        data++;
        size--;
    }
    if (size <= 0) {
        return NULL;
    }
    char *info = malloc(size + 1);
    if (!info) {
        return NULL;
    }
    memcpy(info, data, size);
    if (info[size - 1] == '\n') {
        info[size - 1] = '\0';
    }
    info[size] = '\0';
    return info;
}

/*
 * Reads the model list from an srmodels.bin image at `root`, which must
 * stay in memory: the models' files point into it. ESP-SR's own
 * srmodel_load() in model_path.c does the same.
 */
srmodel_list_t *speaker_srmodel_load(const void *root)
{
    if (static_srmodels) {
        return static_srmodels;
    }
    srmodel_list_t *models = calloc(1, sizeof(*models));
    const char *start = root;
    const char *data = root;
    models->num = read_int32(data);
    data += 4;
    models->model_data = calloc(models->num, sizeof(srmodel_data_t *));
    models->model_name = calloc(models->num, sizeof(char *));
    models->model_info = calloc(models->num, sizeof(char *));
    for (int i = 0; i < models->num; i++) {
        srmodel_data_t *md = calloc(1, sizeof(*md));
        models->model_name[i] = calloc(1, SRMODEL_STRING_LENGTH + 1);
        strncpy(models->model_name[i], data, SRMODEL_STRING_LENGTH);
        data += SRMODEL_STRING_LENGTH;
        md->num = read_int32(data);
        data += 4;
        md->files = calloc(md->num, sizeof(char *));
        md->data = calloc(md->num, sizeof(char *));
        md->sizes = calloc(md->num, sizeof(int));
        for (int j = 0; j < md->num; j++) {
            md->files[j] = (char *)data;
            data += SRMODEL_STRING_LENGTH;
            md->data[j] = (char *)start + read_int32(data);
            data += 4;
            md->sizes[j] = read_int32(data);
            data += 4;
            if (strcmp(md->files[j], "_MODEL_INFO_") == 0) {
                models->model_info[i] = model_info(md->data[j], md->sizes[j]);
            }
        }
        models->model_data[i] = md;
    }
    static_srmodels = models;
    return models;
}

srmodel_list_t *get_static_srmodels(void)
{
    return static_srmodels;
}

/* NULL: models come from memory, not from files under a path. */
char *get_model_base_path(void)
{
    return NULL;
}

int esp_sr_get_debug_mode(void)
{
    return 0;
}

static void vprint(int level, const char *tag, const char *format, va_list args)
{
    char buf[256];
    int n = vsnprintf(buf, sizeof(buf), format, args);
    if (n < 0) {
        return;
    }
    speaker_sr_print(level, tag, buf, n < (int)sizeof(buf) ? (size_t)n : sizeof(buf) - 1);
}

void esp_log_write(esp_log_level_t level, const char *tag, const char *format, ...)
{
    va_list args;
    va_start(args, format);
    vprint(level, tag, format, args);
    va_end(args);
}

int printf(const char *format, ...)
{
    va_list args;
    va_start(args, format);
    vprint(ESP_LOG_NONE, NULL, format, args);
    va_end(args);
    return 0;
}

int fprintf(FILE *f, const char *format, ...)
{
    (void)f;
    va_list args;
    va_start(args, format);
    vprint(ESP_LOG_NONE, NULL, format, args);
    va_end(args);
    return 0;
}

int puts(const char *s)
{
    speaker_sr_print(ESP_LOG_NONE, NULL, s, strlen(s));
    speaker_sr_print(ESP_LOG_NONE, NULL, "\n", 1);
    return 0;
}

int putchar(int c)
{
    char ch = c;
    speaker_sr_print(ESP_LOG_NONE, NULL, &ch, 1);
    return c;
}

/*
 * newlib as built for ESP-IDF asks for the current task's state (errno,
 * locale) here. All callers share newlib's global one: only errno could
 * mix up, and nothing here reads it.
 */
struct _reent *__getreent(void)
{
    return _impure_ptr;
}

/*
 * cJSON only scans numbers back when printing them, to pick the shortest
 * form; failing makes it print 17 digits. Newlib's sscanf would bring in
 * its whole stdio.
 */
int sscanf(const char *str, const char *format, ...)
{
    (void)str;
    (void)format;
    return 0;
}

/*
 * esp-hal's linker script sets the stack guard's address. Defining it here
 * keeps newlib's stack protector, which clashes with esp-hal's, out of the
 * link; the script's value wins.
 */
uint32_t __stack_chk_guard;

/*
 * There are no files: ESP-SR only opens them for models on SPIFFS, and
 * these models are in memory.
 */
FILE *fopen(const char *path, const char *mode)
{
    (void)path;
    (void)mode;
    return NULL;
}

int fclose(FILE *f)
{
    (void)f;
    return -1;
}

size_t fread(void *buf, size_t size, size_t n, FILE *f)
{
    (void)buf;
    (void)size;
    (void)n;
    (void)f;
    return 0;
}

size_t fwrite(const void *buf, size_t size, size_t n, FILE *f)
{
    (void)buf;
    (void)size;
    (void)n;
    (void)f;
    return 0;
}

int fseek(FILE *f, long offset, int whence)
{
    (void)f;
    (void)offset;
    (void)whence;
    return -1;
}

long ftell(FILE *f)
{
    (void)f;
    return -1;
}

/*
 * FreeRTOS mutexes and newlib locks. The detector runs on one task only,
 * so they need not lock anything; a mutex handle only has to be non-NULL.
 */
static int dummy_mutex;

void *xQueueCreateMutex(uint8_t type)
{
    (void)type;
    return &dummy_mutex;
}

long xQueueSemaphoreTake(void *queue, uint32_t ticks)
{
    (void)queue;
    (void)ticks;
    return 1;
}

long xQueueGenericSend(void *queue, const void *item, uint32_t ticks, long position)
{
    (void)queue;
    (void)item;
    (void)ticks;
    (void)position;
    return 1;
}

void vQueueDelete(void *queue)
{
    (void)queue;
}

void _lock_init(_lock_t *lock)
{
    (void)lock;
}

void _lock_acquire(_lock_t *lock)
{
    (void)lock;
}

void _lock_release(_lock_t *lock)
{
    (void)lock;
}
