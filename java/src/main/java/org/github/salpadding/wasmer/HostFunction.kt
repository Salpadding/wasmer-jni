package org.github.salpadding.wasmer

enum class ValType {
    I32, // 0
    I64, // 1
    F32, // 2
    F64;  // 3;

    fun value(): Byte {
        return when(this) {
            I32 -> 0
            I64 -> 1
            F32 -> 2
            F64 -> 4
        }
    }
}

interface HostFunction {
    fun empty(): LongArray {
        return EMPTY
    }

    /**
     * the name of host function
     */
    val name: String

    /**
     * called by webAssembly vm
     */
    fun execute(inst: Instance, args: LongArray): LongArray

    /**
     * function type, Pair.0 = parameters type, Pair.1 = return type
     */
    val signature: Pair<List<ValType>, List<ValType>>


    companion object {
        val EMPTY = LongArray(0)


    }
}