/* Pretends to be ESP-IDF 5.5 for dl_fft. */
#pragma once
#define ESP_IDF_VERSION_VAL(major, minor, patch) (((major) << 16) | ((minor) << 8) | (patch))
#define ESP_IDF_VERSION ESP_IDF_VERSION_VAL(5, 5, 0)
