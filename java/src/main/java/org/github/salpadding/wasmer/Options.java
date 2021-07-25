package org.github.salpadding.wasmer;

public class Options {
    private long threads;
    private long referenceTypes;
    private long simd;
    private long bulkMemory;
    private long multiValue;
    private long tailCall;
    private long moduleLinking;
    private long multiMemory;
    private long memory64;

    public Options threads(boolean threads) {
        this.threads = threads ? 0 : 1L;
        return this;
    }

    public Options referenceTypes(boolean referenceTypes) {
        this.referenceTypes = referenceTypes ? 0 : (1L << 1);
        return this;
    }


    public Options simd(boolean simd) {
        this.simd = simd ? 0 : (1L << 2);
        return this;
    }

    public Options bulkMemory(boolean bulkMemory) {
        this.bulkMemory = bulkMemory ? 0 : (1L << 3);
        return this;
    }

    public Options multiValue(boolean multiValue) {
        this.multiValue = multiValue ? 0 : (1L << 4);
        return this;
    }

    public Options tailCall(boolean tailCall) {
        this.multiValue = tailCall ? 0 : (1L << 5);
        return this;
    }

    public Options moduleLinking(boolean moduleLinking) {
        this.moduleLinking = moduleLinking ? 0 : (1L << 6);
        return this;
    }

    public Options multiMemory(boolean multiMemory) {
        this.multiMemory = multiMemory ? 0 : (1L << 7);
        return this;
    }

    public Options memory64(boolean memory64) {
        this.memory64 = memory64 ? 0 : (1L << 8);
        return this;
    }

    long bitmap() {
        return threads | referenceTypes | simd | bulkMemory | multiValue | tailCall | moduleLinking | multiMemory | memory64;
    }

    private Options() {
    }

    public static Options empty() {
        return new Options();
    }
}
