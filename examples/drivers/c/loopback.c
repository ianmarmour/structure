/*
 * A driver written in C: loopback. The k-th input binding reads back the
 * data last written to the k-th output binding.
 *
 * Build:
 *   cc -shared -fPIC -I crates/structure-runtime/include \
 *      examples/drivers/c/loopback.c -o libloopback.so
 *
 * Use it in a driver configuration with "library": "path/to/libloopback.so".
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "structure_driver.h"

typedef struct {
    size_t count;
    uint32_t *direction;
    size_t *size;
    uint8_t **last_output; /* data of the output bindings */
} loopback;

static void set_error(char *error, size_t error_len, const char *message) {
    if (error != NULL && error_len > 0) {
        snprintf(error, error_len, "%s", message);
    }
}

static void release(loopback *d) {
    for (size_t i = 0; i < d->count; i++) {
        free(d->last_output[i]);
    }
    free(d->direction);
    free(d->size);
    free(d->last_output);
    d->count = 0;
    d->direction = NULL;
    d->size = NULL;
    d->last_output = NULL;
}

static void *lb_create(const char *params_json, char *error, size_t error_len) {
    (void)params_json;
    loopback *d = calloc(1, sizeof *d);
    if (d == NULL) {
        set_error(error, error_len, "out of memory");
    }
    return d;
}

static int lb_open(void *driver, const structure_binding *bindings, size_t count, char *error, size_t error_len) {
    loopback *d = driver;
    release(d);
    d->direction = calloc(count + 1, sizeof *d->direction);
    d->size = calloc(count + 1, sizeof *d->size);
    d->last_output = calloc(count + 1, sizeof *d->last_output);
    if (d->direction == NULL || d->size == NULL || d->last_output == NULL) {
        set_error(error, error_len, "out of memory");
        return -1;
    }
    d->count = count;
    for (size_t i = 0; i < count; i++) {
        d->direction[i] = bindings[i].direction;
        d->size[i] = bindings[i].size;
        if (bindings[i].direction == STRUCTURE_OUTPUT) {
            d->last_output[i] = calloc(bindings[i].size + 1, 1);
            if (d->last_output[i] == NULL) {
                set_error(error, error_len, "out of memory");
                return -1;
            }
        }
    }
    return 0;
}

static int lb_read_inputs(void *driver, const structure_buffer *buffers, size_t count, char *error, size_t error_len) {
    loopback *d = driver;
    if (count != d->count) {
        set_error(error, error_len, "unexpected number of buffers");
        return -1;
    }
    size_t k_in = 0;
    for (size_t i = 0; i < count; i++) {
        if (d->direction[i] != STRUCTURE_INPUT) {
            continue;
        }
        size_t k_out = 0;
        for (size_t j = 0; j < count; j++) {
            if (d->direction[j] != STRUCTURE_OUTPUT) {
                continue;
            }
            if (k_out++ == k_in) {
                size_t n = buffers[i].size < d->size[j] ? buffers[i].size : d->size[j];
                memcpy(buffers[i].data, d->last_output[j], n);
                break;
            }
        }
        k_in++;
    }
    return 0;
}

static int lb_write_outputs(void *driver, const structure_buffer *buffers, size_t count, char *error, size_t error_len) {
    loopback *d = driver;
    if (count != d->count) {
        set_error(error, error_len, "unexpected number of buffers");
        return -1;
    }
    for (size_t i = 0; i < count; i++) {
        if (d->direction[i] == STRUCTURE_OUTPUT) {
            size_t n = buffers[i].size < d->size[i] ? buffers[i].size : d->size[i];
            memcpy(d->last_output[i], buffers[i].data, n);
        }
    }
    return 0;
}

static void lb_destroy(void *driver) {
    loopback *d = driver;
    release(d);
    free(d);
}

static const structure_driver_api api = {
    STRUCTURE_DRIVER_ABI_VERSION,
    lb_create,
    lb_open,
    lb_read_inputs,
    lb_write_outputs,
    NULL, /* stop */
    NULL, /* close */
    lb_destroy,
};

const structure_driver_api *structure_driver_api_v1(void) {
    return &api;
}
