#ifndef COMMONMARKER_COMRAK_FFI_H
#define COMMONMARKER_COMRAK_FFI_H

#include <stddef.h>
#include <stdint.h>

char *commonmarker_call(const uint8_t *input, size_t input_len);
void commonmarker_free_string(char *string);

#endif
