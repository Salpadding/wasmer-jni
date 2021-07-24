package org.github.salpadding.wasmer;

public interface HostFunction {
    long[] EMPTY = new long[0];

    // env
    default String getModule() {
        return "env";
    }

    default long[] empty() {
        return EMPTY;
    }

    String getField();

    long[] execute(Instance inst, long[] args);
}
