/*
Copyright 2022 VMware, Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
*/

/// Backend of the p4c-of compiler.

#ifndef _EXTENSIONS_OFP4_BACKEND_H_
#define _EXTENSIONS_OFP4_BACKEND_H_

#include "ir/ir.h"
#include "frontends/common/options.h"
#include "frontends/common/resolveReferences/referenceMap.h"
#include "frontends/p4/typeMap.h"
#include "options.h"

namespace P4OF {

/// P4 compiler backend for OpenFlow targets.
class BackEnd {
    P4::ReferenceMap* refMap;
    P4::TypeMap*      typeMap;
 public:
    BackEnd(P4::ReferenceMap* refMap, P4::TypeMap* typeMap):
            refMap(refMap), typeMap(typeMap) {}
    void run(P4OFOptions& options, const IR::P4Program* program);
};

}  // namespace P4OF

#endif  /* _EXTENSIONS_OFP4_BACKEND_H_ */