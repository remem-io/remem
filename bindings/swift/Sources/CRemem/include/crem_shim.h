// crem_shim.h — umbrella header for the CRemem Swift Package Manager target.
//
// This target has no C sources of its own. Its only job is to expose the
// rememhq-core C ABI (defined in rememhq.h, copied in from
// rememhq-core/include/rememhq.h — see Sources/CRemem/README.md) as an
// importable Clang module so the Remem Swift target can `import CRemem`.

#ifndef CREM_SHIM_H
#define CREM_SHIM_H

#include "rememhq.h"

#endif // CREM_SHIM_H
