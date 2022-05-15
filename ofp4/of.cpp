/*
Copyright 2022 Vmware, Inc.

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

#include "ir/ir.h"
#include "lib/cstring.h"
#include "lib/stringify.h"

// Implementation of methods for the OF_* IR classes

namespace IR {

const size_t OF_Register::maxRegister = 16;  // maximum register number
const size_t OF_Register::registerSize = 32;  // size of a register in bits
const size_t OF_Register::maxBundleSize = 4;  // xxreg0 has 4 registers, i.e. 128 bits

cstring OF_Register::canonicalName() const {
    cstring result = "";
    size_t n = number;
    for (size_t i = bundle; i > 1; i >>= 1) {
        result += "x";
        n /= 2;
    }
    result += "reg" + Util::toString(n);
    if (high != registerSize * bundle)
        result += "[" + Util::toString(low) + ".." + Util::toString(high) + "]";
    return result;
}

cstring OF_Register::toString() const {
    // If the register has a "friendly" name it is used as a DDlog
    // function name which generates the register name as an
    // interpolated string.
    if (friendlyName)
        return "${r_" + friendlyName + "()}";
    return canonicalName();
}

cstring OF_ResubmitAction::toString() const {
    return cstring("resubmit(,") + Util::toString(nextTable) + ")";
}

cstring OF_Constant::toString() const {
    bool isSigned = false;
    if (auto tb = value->type->to<IR::Type_Bits>())
        isSigned = tb->isSigned;
    return Util::toString(value->value, 0, isSigned, value->base);
}

cstring OF_TableMatch::toString() const {
    return "table=" + Util::toString(id);
}

cstring OF_ProtocolMatch::toString() const {
    return proto + ",";
}

cstring OF_EqualsMatch::toString() const {
    return left->toString() + "=" + right->toString();
}

cstring OF_Slice::toString() const {
    return base->toString() + "[" + Util::toString(low) + ".." + Util::toString(high) + "]";
}

cstring OF_MatchAndAction::toString() const {
    return match->toString() + " actions=(" + action->toString() + ")";
}

cstring OF_MoveAction::toString() const {
    return "move(" + src->toString() + "->" + dest->toString() + ")";
}

cstring OF_LoadAction::toString() const {
    return "load(" + src->toString() + "->" + dest->toString() + ")";
}

cstring OF_SeqAction::toString() const {
    if (left->is<IR::OF_EmptyAction>())
        return right->toString();
    if (right->is<IR::OF_EmptyAction>())
        return left->toString();
    return left->toString() + ", " + right->toString();
}

class OpenFlowSimplify : public Transform {
    bool foundResubmit = false;

 public:
    OpenFlowSimplify() { setName("OpenFlowSimplify"); }

    const IR::Node* postorder(IR::OF_Slice* slice) override {
        if (auto br = slice->base->to<IR::OF_Register>()) {
            // convert the slice of a register into a register
            return new IR::OF_Register(
                br->number, br->low + slice->low, br->low + slice->high, br->bundle);
        }
        return slice;
    }

    const IR::Node* postorder(IR::OF_Action* action) override {
        if (foundResubmit)
            return new IR::OF_EmptyAction();
        return action;
    }

    const IR::Node* postorder(IR::OF_SeqAction* sequence) override {
        if (sequence->left->is<IR::OF_EmptyAction>())
            return sequence->right;
        if (sequence->right->is<IR::OF_EmptyAction>())
            return sequence->left;
        return sequence;
    }

    const IR::Node* postorder(IR::OF_ResubmitAction* action) override {
        if (foundResubmit)
            return new IR::OF_EmptyAction();
        foundResubmit = true;
        return action;
    }
};

}   // namespace IR