package org.github.salpadding.wasmer;

public class InstanceOptions {
    private long threads;
    private long referenceTypes;
    private long simd;
    private long bulkMemory;
    private long multiValue;
    private long tailCall;
    private long moduleLinking;
    private long multiMemory;
    private long memory64;
    private long disableStartSection;
    private long forceMemoryExport;

    public InstanceOptions threads(boolean threads) {
        this.threads = threads ? 0 : 1L;
        return this;
    }

    public InstanceOptions referenceTypes(boolean referenceTypes) {
        this.referenceTypes = referenceTypes ? 0 : (1L << 1);
        return this;
    }


    public InstanceOptions simd(boolean simd) {
        this.simd = simd ? 0 : (1L << 2);
        return this;
    }

    public long bitmap() {
        return threads | referenceTypes | simd | bulkMemory | multiValue | tailCall | moduleLinking | multiMemory | memory64;
    }

    private InstanceOptions() {
    }

    public static InstanceOptions empty() {
        return new InstanceOptions();
    }
}
