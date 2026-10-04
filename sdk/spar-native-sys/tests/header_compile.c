#include "spar_native.h"
int main(void){ SparValue v = spar_int(3); return v.tag == SPAR_TAG_INT ? 0 : 1; }
