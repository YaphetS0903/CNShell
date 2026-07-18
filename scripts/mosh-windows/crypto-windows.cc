/* Use Windows CNG for entropy while retaining Mosh's upstream AES-OCB protocol code. */

#include "config.h"
#include "crypto.h"
#include <bcrypt.h>

#define PRNG_HPP
class PRNG {
private:
  PRNG(const PRNG &);
  PRNG &operator=(const PRNG &);

public:
  PRNG() {}

  void fill(void *destination, size_t size) {
    if (size == 0) return;
    if (!destination || size > ULONG_MAX ||
        BCryptGenRandom(nullptr, static_cast<PUCHAR>(destination),
                        static_cast<ULONG>(size), BCRYPT_USE_SYSTEM_PREFERRED_RNG) != 0) {
      throw Crypto::CryptoException("Windows system random generator failed.");
    }
  }

  uint8_t uint8() { uint8_t value; fill(&value, sizeof(value)); return value; }
  uint32_t uint32() { uint32_t value; fill(&value, sizeof(value)); return value; }
  uint64_t uint64() { uint64_t value; fill(&value, sizeof(value)); return value; }
};

#include "crypto.cc"
