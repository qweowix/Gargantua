package app.blackhole

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** Решатель подписи: деление плеера на операторы и сам прогон — на игрушечном «плеере» той же формы, что у YouTube */
class YtSolverTest {
    // своя модель: класс URL, у которого первый «свой» метод переворачивает n, и функция ссылки с set("alr","yes"),
    // которая переворачивает подпись. Вызов с побочкой и помеченный блок — как в настоящем плеере
    private val toy = """var _yt_player={};(function(g){var window=this;/* c */'use strict';var da,U,IL,other,lbl;
da="set;get;n".split(";");
a:{var lbl=1;if(lbl)lbl=2}
g.U=function(a,b){this.p={};this.p.url=a};g.k=g.U.prototype;
g.k.set=function(k,v){this.p[k]=v};g.k.get=function(k){return this.p[k]};
g.k.Xn=function(){var n=this.p[da[2]];if(n)this.p[da[2]]=n.split("").reverse().join("")+lbl};g.k.clone=function(){};
IL=function(a,b,c){b=b===void 0?"":b;a=new g.U(a,!0);a.set("alr","yes");c&&(c=decodeURIComponent(c).split("").reverse().join(""),a.set(b,encodeURIComponent(c)));return a};
other=function(a){a.get("alr")||a.set("alr","yes");return a};
sideEffect();new g.U("x");var re=/[;{}]/g,tpl=`a${'$'}{1+{}.x}b`;
function decl(){return 1}
if(lbl){var z=3}else{z=4}
})(_yt_player);
"""

    @Test
    fun splitsAndKeeps() {
        val start = toy.indexOf("(function(g){") + 13
        val st = YtSolver.statements(toy, start, toy.lastIndexOf("})(_yt_player)")).map { (b, e) -> toy.substring(b, e) }
        assertTrue(st.any { it.startsWith("a:{var lbl=1") && it.endsWith("}") })
        assertTrue(st.contains("sideEffect();"))
        assertTrue(st.any { it.startsWith("if(lbl){var z=3}else{z=4}") })
        assertTrue(st.contains("function decl(){return 1}"))
        assertFalse(YtSolver.keep("sideEffect();"))
        assertFalse(YtSolver.keep("new g.U(\"x\");"))
        assertTrue(YtSolver.keep("a:{var lbl=1;if(lbl)lbl=2}"))
        assertTrue(YtSolver.isAssignment("g.k.set=function(k,v){this.p[k]=v};"))
        assertFalse(YtSolver.isAssignment("a==b;"))
        assertFalse(YtSolver.isAssignment("a=1,b=2;"))
        assertTrue(YtSolver.isAssignment("a+=f(x,y);"))
        assertEquals("IL", st.firstNotNullOfOrNull { YtSolver.alrName(it) })
        assertEquals(listOf("IL"), st.mapNotNull { YtSolver.alrName(it) })
    }

    @Test
    fun solvesToyPlayer() {
        val (sc, fn) = YtSolver.load(YtSolver.prepare(toy), "toy")
        val (sig, n) = YtSolver.call(sc, fn, "abc=def", "xyz")
        assertEquals("fed=cba", sig)
        assertEquals("zyx2", n)
        assertEquals("zyx2", YtSolver.call(sc, fn, null, "xyz").second)
    }

    /** Настоящий плеер, если лежит рядом (только текст: делим и ищем функцию, ничего не исполняем) */
    @Test
    fun realPlayerSplits() {
        val f = File(System.getenv("YT_PLAYER") ?: return)
        if (!f.isFile) return
        val js = f.readText()
        val start = js.indexOf("(function(g){") + 13
        val st = YtSolver.statements(js, start, js.lastIndexOf("})(_yt_player)")).map { (b, e) -> js.substring(b, e) }
        println("операторов: ${st.size}, оставлено: ${st.count { YtSolver.keep(it) }}, кандидаты: ${st.mapNotNull { YtSolver.alrName(it) }}")
        assertEquals(listOf("IL"), st.mapNotNull { YtSolver.alrName(it) })
    }
}
