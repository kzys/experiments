/* newlib's sys/lock.h plus ESP-IDF's _lock_* API, for dl_fft. */
#pragma once
#include_next <sys/lock.h>
typedef int _lock_t;
void _lock_init(_lock_t *lock);
void _lock_acquire(_lock_t *lock);
void _lock_release(_lock_t *lock);
