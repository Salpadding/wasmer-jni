package com.github.salpadding.wasmer;

import java.util.List;

public interface HostFunction {
    long[] EMPTY_LONGS = new long[0];

    /**
     * the name of host function
     */
    String getName();

    /**
     * called by webAssembly vm
     */
    long[] execute(Instance ins, long[] args);

    /**
     * function type = parameters type + return type
     */
    List<ValType> getParams();

    List<ValType> getRet();
}
