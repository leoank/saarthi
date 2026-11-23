package me.leoank.saarthi

import androidx.compose.ui.graphics.Color
import androidx.compose.foundation.layout.wrapContentHeight
import androidx.compose.ui.unit.sp
import androidx.compose.ui.graphics.Brush
import androidx.compose.foundation.background
import androidx.compose.ui.unit.Dp
import androidx.compose.material3.Surface
import androidx.compose.material3.Checkbox
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import uniffi.saarthi_core.PendingInfo
import uniffi.saarthi_core.Question

/** A request waiting on the person: a permission to allow or deny, or questions to answer.
 *  [where] names the session when the card is shown away from it. Its body scrolls within
 *  [maxBody], so a long command or many options never push its buttons out of reach. */
@Composable
fun PendingCard(
    model: AgentsModel,
    p: PendingInfo,
    where: String?,
    modifier: Modifier = Modifier,
    maxBody: Dp = Dp.Unspecified,
) {
    val question = p.questions.isNotEmpty()
    Surface(
        modifier.fillMaxWidth(),
        shape = RoundedCornerShape(6.dp),
        color = Flame.S0,
        border = BorderStroke(1.dp, Flame.Hot.copy(alpha = 0.45f)),
    ) {
        Column(
            Modifier
                .background(androidx.compose.ui.graphics.Brush.verticalGradient(listOf(Flame.soft(Flame.Hot, 0.07f), Flame.S0)))
                .padding(14.dp),
        ) {
            // `── claude asks ──` (ADR-0069).
            Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.padding(bottom = 6.dp)) {
                Text("── ", fontFamily = Mono, fontSize = 11.sp, color = Flame.Hot.copy(alpha = 0.6f))
                Text(if (question) "claude asks" else "allow?", fontFamily = Mono, fontSize = 11.sp, fontWeight = FontWeight.SemiBold, color = Flame.Hot)
                Text(" ──", fontFamily = Mono, fontSize = 11.sp, color = Flame.Hot.copy(alpha = 0.6f))
            }
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(
                    if (question) "Claude asks" else "Allow ${p.toolName}?",
                    style = MaterialTheme.typography.titleMedium,
                    modifier = Modifier.weight(1f),
                )
            }
            val sub = listOfNotNull(where, relative(p.createdMs)).joinToString(" · ")
            Text(sub, style = MonoMeta, color = MaterialTheme.colorScheme.onSurfaceVariant)
            if (question) QuestionForm(p.questions, maxBody) { answers -> model.answer(p, answers) }
            else PermissionForm(p, maxBody, allow = { model.allow(p) }, deny = { model.deny(p, it) })
        }
    }
}

@Composable
private fun PermissionForm(p: PendingInfo, maxBody: Dp, allow: () -> Unit, deny: (String) -> Unit) {
    var asking by remember(p.id) { mutableStateOf(false) }
    Text(
        p.summary.ifBlank { p.toolInputJson },
        fontFamily = Mono,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurface,
        modifier = Modifier
            .fillMaxWidth()
            .padding(vertical = 12.dp)
            .capped(maxBody)
            .background(MaterialTheme.colorScheme.surface, RoundedCornerShape(8.dp))
            .padding(10.dp),
    )
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(onClick = allow, modifier = Modifier.weight(1f)) { Text("Allow") }
        OutlinedButton(onClick = { asking = true }, modifier = Modifier.weight(1f)) { Text("Deny") }
    }
    if (asking) {
        var message by remember { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { asking = false },
            title = { Text("Deny ${p.toolName}") },
            text = {
                Column {
                    Text("Claude sees the reason and can try another way.", style = MaterialTheme.typography.bodyMedium)
                    OutlinedTextField(
                        message, { message = it },
                        label = { Text("Reason (optional)") },
                        modifier = Modifier.fillMaxWidth().padding(top = 12.dp),
                    )
                }
            },
            confirmButton = { TextButton(onClick = { asking = false; deny(message.trim()) }) { Text("Deny") } },
            dismissButton = { TextButton(onClick = { asking = false }) { Text("Cancel") } },
        )
    }
}

/** At most [max] tall, scrolling within; unchanged without a cap, as in a list that scrolls
 *  itself (a scroll inside one has no height to scroll within). */
@Composable
private fun Modifier.capped(max: Dp): Modifier =
    if (max == Dp.Unspecified) this else heightIn(max = max).verticalScroll(rememberScrollState())

/** Claude marks the option it recommends with this, as Claude Code's dialog shows it. */
private const val RECOMMENDED = "(Recommended)"

/** One block per question: numbered option rows with their descriptions, a *Recommended*
 *  pill where Claude marked one, checkboxes and *Submit (n)* for a multi-select, and an
 *  answer of the person's own. A single question with a single answer goes with one tap. */
@Composable
private fun QuestionForm(questions: List<Question>, maxBody: Dp, submit: (Map<String, List<String>>) -> Unit) {
    val chosen = remember(questions) { mutableStateMapOf<String, List<String>>() }
    val own = remember(questions) { mutableStateMapOf<String, String>() }
    var typing by remember(questions) { mutableStateOf(false) }
    val oneTap = questions.size == 1 && !questions[0].multiSelect
    Column(
        Modifier
            .padding(top = 8.dp)
            .capped(maxBody),
    ) {
        for (q in questions) {
            if (q.header.isNotBlank()) {
                Text(
                    q.header.uppercase(),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = 8.dp),
                )
            }
            Text(q.question, fontWeight = FontWeight.SemiBold, modifier = Modifier.padding(top = 2.dp, bottom = 6.dp))
            q.options.forEachIndexed { i, option ->
                val picked = option in chosen[q.question].orEmpty()
                OptionRow(
                    number = i + 1,
                    option = option,
                    description = q.descriptions.getOrNull(i).orEmpty(),
                    picked = picked,
                    multi = q.multiSelect,
                ) {
                    own[q.question] = ""
                    if (oneTap) {
                        submit(mapOf(q.question to listOf(option)))
                        return@OptionRow
                    }
                    val now = chosen[q.question].orEmpty()
                    chosen[q.question] = when {
                        picked -> now - option
                        q.multiSelect -> now + option
                        else -> listOf(option)
                    }
                }
            }
            if (typing) {
                OutlinedTextField(
                    own[q.question].orEmpty(), { own[q.question] = it; if (it.isNotBlank()) chosen.remove(q.question) },
                    label = { Text("Your answer") },
                    modifier = Modifier.fillMaxWidth().padding(top = 6.dp),
                )
            }
        }
    }
    if (!typing) {
        TextButton(onClick = { typing = true }, contentPadding = PaddingValues(horizontal = 4.dp)) {
            Text("Or type your own answer")
        }
    }
    val answered = questions.all { q -> own[q.question].orEmpty().isNotBlank() || chosen[q.question].orEmpty().isNotEmpty() }
    if (!oneTap || typing) {
        val count = questions.sumOf { q -> if (own[q.question].orEmpty().isNotBlank()) 1 else chosen[q.question].orEmpty().size }
        Button(
            onClick = {
                submit(questions.associate { q ->
                    val typed = own[q.question].orEmpty().trim()
                    q.question to if (typed.isNotEmpty()) listOf(typed) else chosen[q.question].orEmpty()
                })
            },
            enabled = answered,
            modifier = Modifier.fillMaxWidth().padding(top = 4.dp),
        ) { Text(if (questions.size == 1 && questions[0].multiSelect) "Submit ($count)" else "Answer") }
    }
}

/** One option: its number, its label, a *Recommended* pill, and what it means beneath. */
@Composable
private fun OptionRow(number: Int, option: String, description: String, picked: Boolean, multi: Boolean, onClick: () -> Unit) {
    val recommended = option.trimEnd().endsWith(RECOMMENDED)
    val label = if (recommended) option.trimEnd().removeSuffix(RECOMMENDED).trimEnd() else option
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(4.dp),
        color = if (picked) Flame.soft(Flame.Hot, 0.12f) else if (recommended) Flame.soft(Flame.Hot, 0.06f) else Color.Transparent,
        border = BorderStroke(1.dp, if (picked) Flame.Hot.copy(alpha = 0.6f) else Flame.Line),
        modifier = Modifier.fillMaxWidth().padding(vertical = 3.dp),
    ) {
        Row(verticalAlignment = Alignment.Top, modifier = Modifier.padding(horizontal = 12.dp, vertical = 10.dp)) {
            if (multi) {
                Checkbox(checked = picked, onCheckedChange = { onClick() }, modifier = Modifier.size(20.dp).padding(top = 2.dp))
                Spacer(Modifier.width(10.dp))
            }
            Text(
                "$number",
                fontFamily = Mono,
                fontWeight = FontWeight.Bold,
                color = Flame.Hot,
                modifier = Modifier.width(28.dp),
            )
            Column(Modifier.weight(1f)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(label, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f, fill = false))
                    if (recommended) {
                        Spacer(Modifier.width(8.dp))
                        Text("recommended", fontFamily = Mono, fontSize = 10.5.sp, color = Flame.Dim)
                    }
                }
                if (description.isNotBlank()) {
                    Text(description, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
        }
    }
}

/** An option chosen by its number, as typed in the prompt bar: the single question of [p]
 *  answered with option [text], or in the person's own words when [text] is not a number of
 *  one. Null when [p] is not one question with a single answer. */
fun typedAnswer(p: PendingInfo, text: String): Map<String, List<String>>? {
    val q = p.questions.singleOrNull()?.takeIf { !it.multiSelect } ?: return null
    val n = text.trim().removeSuffix(".").toIntOrNull()
    val option = n?.let { q.options.getOrNull(it - 1) }
    return mapOf(q.question to listOf(option ?: text.trim()))
}
