# wasmer-jni

wasmer jni binding, support host function, memory read/write

TODO:

1. support metering 
2. module validation
3. memory/frame/stack and other resources limitaion
4. multiple compiler choice, current is single



## How to use?

1. add wasmer_jni.jar to your project

2. example code: 

```java
package com.github.salpadding.wasmer.example;


import com.github.salpadding.wasmer.*;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;


class MemoryPeek implements HostFunction {
    @Override
    public String getName() {
        return "__peek";
    }

    @Override
    public long[] execute(Instance inst, long[] args) {
        int off = (int) args[0];
        int len = (int) args[1];

        byte[] data = inst.getMemory("memory").read(off, len);

        for (byte b : data) {
            System.out.print(Integer.toString(b & 0xff, 16));
        }

        System.out.println();
        return Instance.EMPTY_LONGS;
    }

    @Override
    public List<ValType> getParams() {
        return Arrays.asList(ValType.I32, ValType.I32);
    }

    @Override
    public List<ValType> getRet() {
        return Collections.emptyList();
    }
}

class EmptyHost implements HostFunction {
    private final String name;

    public EmptyHost(String name) {
        this.name = name;
    }

    @Override
    public String getName() {
        return name;
    }

    @Override
    public long[] execute(Instance inst, long[] args) {
        System.out.println("empty host function executed");
        return EMPTY_LONGS;
    }

    @Override
    public List<ValType> getParams() {
        return Collections.singletonList(ValType.I64);
    }

    @Override
    public List<ValType> getRet() {
        return Collections.emptyList();
    }
}

public class Example {
    public static void main(String[] args) {
        Natives.initialize(1024);
        byte[] bin = TestUtil.readClassPathFile("testdata/wasm.wasm");
        Instance ins = Instance.create(bin, Options.empty(), Arrays.asList(new EmptyHost("alert"), new MemoryPeek()));

        try {
            // for Integer, use Integer.toUnsignedLong
            // for Float, use Float.floatToIntBits + Integer.toUnsignedLong
            // for Double, use Double.doubleToLongBits
            long[] params = new long[2];
            params[0] = Long.MAX_VALUE;
            params[1] = Integer.toUnsignedLong(Integer.MAX_VALUE);
            ins.execute("init", params);
        } finally {
            ins.close();
        }
    }
}

```