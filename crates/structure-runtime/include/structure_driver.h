/*
 * structure_driver.h - C interface of I/O drivers for the structure
 * IEC 61131-3 runtime (driver ABI version 1).
 *
 * A driver is a shared library (.so, .dylib, .dll) that exports
 *
 *     const structure_driver_api *structure_driver_api_v1(void);
 *
 * returning a table of functions that stays valid while the library is
 * loaded. The runtime:
 *
 *   1. calls create() once with the "params" object of the driver
 *      configuration (JSON text);
 *   2. calls open() when the configuration starts (cold or warm restart),
 *      with the bindings of the driver: the parts of the process image of
 *      the resource that the driver exchanges;
 *   3. calls read_inputs() when a task of the resource is activated (before
 *      its programs execute) and write_outputs() when the task has completed;
 *      a driver associated with a task in the configuration is only called
 *      around the executions of that task;
 *   4. calls stop() when the resource stops (fault or shutdown): the driver
 *      should bring the outputs into a safe state;
 *   5. calls close(), then destroy() when the configuration is shut down.
 *
 * All functions of an instance are called from one thread at a time (the
 * scheduling thread of the runtime). read_inputs() and write_outputs() are
 * called in every task cycle and should not block for longer than the
 * exchange takes; a driver for slow devices typically polls them from a
 * thread of its own and only copies buffers in these functions.
 *
 * Process image layout: the bytes of an area are those of the directly
 * represented variables of IEC 61131-3. %IXa.b is bit b (LSB = 0) of byte
 * a; %IBn is byte n; %IWn is bytes 2n..2n+1, %IDn bytes 4n..4n+3 and %ILn
 * bytes 8n..8n+7, all little-endian. For an area with a module path
 * (binding "%I1"), the same applies to the last address fields: %IW1.2 is
 * bytes 4..5 of the area of module 1.
 *
 * Functions returning int return 0 on success. On failure they return a
 * nonzero value and may write a NUL-terminated message of at most
 * error_len bytes (including the NUL) to error.
 */
#ifndef STRUCTURE_DRIVER_H
#define STRUCTURE_DRIVER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#define STRUCTURE_DRIVER_ABI_VERSION 1u

/* Direction of a binding. */
#define STRUCTURE_INPUT 0u  /* device -> process image (%I) */
#define STRUCTURE_OUTPUT 1u /* process image -> device (%Q) */

/* A part of the process image exchanged by the driver. The pointers are
 * valid until the next call of open() or until destroy(). */
typedef struct structure_binding {
    const char *area;        /* as configured: "%I", "%Q1", "%I2.1" */
    uint32_t direction;      /* STRUCTURE_INPUT or STRUCTURE_OUTPUT */
    const uint32_t *path;    /* module path ({1} for "%I1"); NULL if empty */
    size_t path_len;
    size_t offset;           /* byte offset of the binding in the area */
    size_t size;             /* size in bytes; 0 if the configuration uses
                                no variable of the area */
    const char *params_json; /* "params" of the binding (JSON text) */
} structure_binding;

/* The process image of a binding during read_inputs/write_outputs. The
 * buffers are given in the order of the bindings passed to open(); the
 * buffers of bindings of the other direction are empty (size 0). */
typedef struct structure_buffer {
    uint8_t *data;
    size_t size;
} structure_buffer;

typedef struct structure_driver_api {
    /* STRUCTURE_DRIVER_ABI_VERSION */
    uint32_t abi_version;

    /* Creates an instance; returns NULL on failure. */
    void *(*create)(const char *params_json, char *error, size_t error_len);

    int (*open)(void *driver, const structure_binding *bindings, size_t count,
                char *error, size_t error_len);

    /* Stores the current inputs into the buffers of the input bindings. */
    int (*read_inputs)(void *driver, const structure_buffer *buffers,
                       size_t count, char *error, size_t error_len);

    /* Sends the buffers of the output bindings; the buffers shall not be
     * modified. */
    int (*write_outputs)(void *driver, const structure_buffer *buffers,
                         size_t count, char *error, size_t error_len);

    /* Optional (may be NULL). */
    void (*stop)(void *driver);
    void (*close)(void *driver);

    void (*destroy)(void *driver);
} structure_driver_api;

/* Exported by every driver library. */
const structure_driver_api *structure_driver_api_v1(void);

#ifdef __cplusplus
}
#endif

#endif /* STRUCTURE_DRIVER_H */
