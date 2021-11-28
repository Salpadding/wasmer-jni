package com.github.salpadding.wasmer;

public enum ValType {
    I32, // 0
    I64, // 1
    F32, // 2
    F64;

    byte value() {
        switch (this) {
            case I32:
                return 0;
            case I64:
                return 1;
            case F32:
                return 2;
            case F64:
                return 4;
        }
        return 0;
    }
}
