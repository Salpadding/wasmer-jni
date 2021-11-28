package org.github.salpadding.wasmer

import org.github.salpadding.wasmer.Instance.Companion.create
import org.junit.runner.RunWith
import org.junit.runners.JUnit4
import org.junit.Test


class MemoryPeek: HostFunction {
    override val name: String
        get() = "__peek"

    override fun execute(inst: Instance, args: LongArray): LongArray {
        val off = args[0].toInt()
        val len = args[1].toInt()

        val data = inst.getMemory().read(off, len)
        println(data.map { String.format("%02x", it) })

        inst.getMemory().write(off, byteArrayOf(0, 0, 0, 1))

        return empty()
    }

    override val signature: Pair<List<ValType>, List<ValType>>
        get() = Pair(listOf(ValType.I32, ValType.I32), emptyList())

}

class EmptyHost(override val name: String) : HostFunction {
    override fun execute(inst: Instance, args: LongArray): LongArray {
       println("======= host function executed " + args[0])
        return empty()
    }

    override val signature: Pair<List<ValType>, List<ValType>>
        get() = Pair(listOf(ValType.I64), emptyList())
}

@RunWith(JUnit4::class)
class WasmTest {


    @Test
    fun test1() {
        Natives.initialize(8);
        val bin = TestUtil.readClassPathFile("testdata/wasm.wasm")

        create(bin, Options.empty(), listOf(EmptyHost("alert"), MemoryPeek())).use {
            val start = System.currentTimeMillis()
            for (i in 0 until LOOP) {
                it.execute("init", longArrayOf(Long.MAX_VALUE, Int.MAX_VALUE.toLong()))
            }
            val end = System.currentTimeMillis()
            println("ops = " + LOOP * 1.0 / (end - start) * 1000)
        }

    }

    companion object {
        const val LOOP = 1
    }
}