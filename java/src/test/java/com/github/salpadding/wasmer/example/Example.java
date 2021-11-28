package com.github.salpadding.wasmer.example;


import com.github.salpadding.wasmer.*;
import kotlin.Pair;
import org.jetbrains.annotations.NotNull;

import java.util.Arrays;
import java.util.Collections;
import java.util.List;


class MemoryPeek implements HostFunction {
    @NotNull
    @Override
    public String getName() {
        return "__peek";
    }

    @NotNull
    @Override
    public long[] execute(@NotNull Instance inst, @NotNull long[] args) {
        int off = (int) args[0];
        int len = (int) args[1];

        byte[] data = inst.getMemory("memory").read(off, len);

        for (byte b : data) {
            System.out.print(Integer.toString(b & 0xff, 16));
        }

        System.out.println();
        return HostFunction.getEmptyLongs();
    }

    @NotNull
    @Override
    public Pair<List<ValType>, List<ValType>> getSignature() {
        return new Pair<>(
                Arrays.asList(ValType.I32, ValType.I32),
                Collections.emptyList()
        );
    }
}

class EmptyHost implements HostFunction {
    private String name;

    public EmptyHost(String name) {
        this.name = name;
    }

    @NotNull
    @Override
    public String getName() {
        return name;
    }

    @NotNull
    @Override
    public long[] execute(@NotNull Instance inst, @NotNull long[] args) {
        System.out.println("empty host function executed");
        return HostFunction.getEmptyLongs();
    }

    @NotNull
    @Override
    public Pair<List<ValType>, List<ValType>> getSignature() {
        return new Pair<>(Collections.singletonList(ValType.I64), Collections.emptyList());
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
