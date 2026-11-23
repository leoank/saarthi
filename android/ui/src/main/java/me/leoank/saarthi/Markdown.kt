package me.leoank.saarthi

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.compositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp

/** The words a search in the session looks for, marked wherever text shows them; empty
 *  when nothing is searched. */
val LocalFind = compositionLocalOf { "" }

/** [text] with each place it has one of the searched words marked in the flame's amber. */
@Composable
fun found(text: AnnotatedString): AnnotatedString {
    // Each word on its own, as the hub's search matches them.
    val words = LocalFind.current.split(' ').filter { it.length >= 2 }
    if (words.isEmpty()) return text
    val mark = SpanStyle(background = Flame.Hot.copy(alpha = 0.38f), color = Flame.Text)
    var b: AnnotatedString.Builder? = null
    for (w in words) {
        var i = text.text.indexOf(w, ignoreCase = true)
        while (i >= 0) {
            (b ?: AnnotatedString.Builder(text).also { b = it }).addStyle(mark, i, i + w.length)
            i = text.text.indexOf(w, i + w.length, ignoreCase = true)
        }
    }
    return b?.toAnnotatedString() ?: text
}

@Composable
fun found(text: String): AnnotatedString = found(AnnotatedString(text))

/** Enough markdown for what Claude writes: headings, bullet and numbered lists, fenced
 *  code, tables and quotes as blocks; code, bold, italic and links inline. Anything else is
 *  shown as it is, never hidden. */
@Composable
fun Markdown(text: String, modifier: Modifier = Modifier, style: TextStyle = MaterialTheme.typography.bodyLarge) {
    Column(modifier) {
        for (block in blocks(text)) when (block) {
            is Block.Code -> CodeBlock(block.text)
            is Block.Heading -> Text(
                inline(block.text),
                style = if (block.level <= 2) MaterialTheme.typography.titleMedium else MaterialTheme.typography.titleSmall,
                modifier = Modifier.padding(top = 8.dp, bottom = 2.dp),
            )
            is Block.ListItem -> Row(Modifier.padding(start = (12 * block.depth).dp, top = 2.dp)) {
                Text(block.marker, style = style, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Spacer(Modifier.width(8.dp))
                Text(inline(block.text), style = style)
            }
            is Block.Quote -> Row(Modifier.padding(vertical = 2.dp)) {
                Spacer(Modifier.width(3.dp).padding(0.dp).background(MaterialTheme.colorScheme.outlineVariant))
                Text(
                    inline(block.text),
                    style = style,
                    fontStyle = FontStyle.Italic,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 10.dp),
                )
            }
            is Block.Paragraph -> Text(inline(block.text), style = style, modifier = Modifier.padding(vertical = 3.dp))
        }
    }
}

@Composable
fun CodeBlock(text: String, modifier: Modifier = Modifier) {
    val wrap = LocalWrap.current.on
    Text(
        found(text),
        fontFamily = Mono,
        style = MaterialTheme.typography.bodySmall,
        softWrap = wrap,
        modifier = modifier
            .fillMaxWidth()
            .padding(vertical = 4.dp)
            .background(MaterialTheme.colorScheme.surfaceContainerHigh, RoundedCornerShape(8.dp))
            .padding(10.dp)
            .scrollUnlessWrapped(wrap),
    )
}

private sealed interface Block {
    data class Paragraph(val text: String) : Block
    data class Heading(val level: Int, val text: String) : Block
    data class ListItem(val marker: String, val depth: Int, val text: String) : Block
    data class Code(val text: String) : Block
    data class Quote(val text: String) : Block
}

private val bullet = Regex("""^(\s*)[-*+] (.*)$""")
private val numbered = Regex("""^(\s*)(\d+)[.)] (.*)$""")
private val heading = Regex("""^(#{1,6}) +(.*)$""")

private fun blocks(text: String): List<Block> {
    val out = mutableListOf<Block>()
    val lines = text.lines()
    var i = 0
    val para = StringBuilder()
    fun flush() {
        if (para.isNotEmpty()) {
            out += Block.Paragraph(para.toString().trim())
            para.clear()
        }
    }
    while (i < lines.size) {
        val line = lines[i]
        when {
            line.trimStart().startsWith("```") -> {
                // A fence closes on a bare ``` only, so a fenced sample inside a fence stays
                // in it; an empty block (a stray closing fence) is dropped.
                flush()
                val code = StringBuilder()
                i++
                while (i < lines.size && lines[i].trim() != "```") {
                    code.appendLine(lines[i]); i++
                }
                val body = code.toString().trimEnd()
                if (body.isNotEmpty()) out += Block.Code(body)
            }
            line.startsWith("|") && line.trimEnd().endsWith("|") -> {
                flush()
                val rows = StringBuilder()
                while (i < lines.size && lines[i].startsWith("|")) {
                    val row = lines[i]
                    if (!row.matches(Regex("""^\|[\s:|-]+\|$"""))) rows.appendLine(row)
                    i++
                }
                out += Block.Code(rows.toString().trimEnd())
                continue
            }
            heading.matches(line) -> {
                flush()
                val m = heading.find(line)!!
                out += Block.Heading(m.groupValues[1].length, m.groupValues[2])
            }
            bullet.matches(line) -> {
                flush()
                val m = bullet.find(line)!!
                out += Block.ListItem("•", m.groupValues[1].length / 2, m.groupValues[2])
            }
            numbered.matches(line) -> {
                flush()
                val m = numbered.find(line)!!
                out += Block.ListItem(m.groupValues[2] + ".", m.groupValues[1].length / 2, m.groupValues[3])
            }
            line.startsWith("> ") || line == ">" -> {
                flush()
                out += Block.Quote(line.removePrefix(">").trim())
            }
            line.isBlank() -> flush()
            else -> {
                // A line break inside a paragraph stays one, as the terminal shows it.
                if (para.isNotEmpty()) para.append('\n')
                para.append(line.trim())
            }
        }
        i++
    }
    flush()
    return out
}

private val inlineToken = Regex("""(`[^`]+`)|(\*\*.+?\*\*)|(\*[^*\s][^*]*\*)|(_[^_\s][^_]*_)|(\[[^\]]+]\([^)]+\))""")

@Composable
private fun inline(text: String): AnnotatedString {
    val codeBg = MaterialTheme.colorScheme.surfaceContainerHigh
    val link = MaterialTheme.colorScheme.primary
    // Bold and italic may wrap code or a link, so their insides go through the same rules.
    fun androidx.compose.ui.text.AnnotatedString.Builder.spans(text: String) {
        var last = 0
        for (m in inlineToken.findAll(text)) {
            append(text.substring(last, m.range.first))
            val t = m.value
            when {
                t.startsWith("`") -> withStyle(SpanStyle(fontFamily = Mono, background = codeBg)) {
                    append(t.trim('`'))
                }
                t.startsWith("**") -> withStyle(SpanStyle(fontWeight = FontWeight.SemiBold)) { spans(t.removeSurrounding("**")) }
                t.startsWith("*") || t.startsWith("_") -> withStyle(SpanStyle(fontStyle = FontStyle.Italic)) {
                    spans(t.substring(1, t.length - 1))
                }
                else -> withStyle(SpanStyle(color = link)) { append(t.substring(1, t.indexOf(']'))) }
            }
            last = m.range.last + 1
        }
        append(text.substring(last))
    }
    return found(buildAnnotatedString { spans(text) })
}
