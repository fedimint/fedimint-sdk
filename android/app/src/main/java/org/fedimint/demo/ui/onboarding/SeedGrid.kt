package org.fedimint.demo.ui.onboarding

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.VerticalDivider
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import org.fedimint.demo.ui.common.CardDivider
import org.fedimint.demo.ui.common.WalletCard
import org.fedimint.demo.ui.theme.WalletText

/**
 * Numbered word slots in two columns, read left to right then down (01 02,
 * 03 04, …), as the design lays them out. [cell] draws the word or field
 * for a zero-based position.
 */
@Composable
fun SeedGrid(count: Int, cell: @Composable RowScope.(index: Int) -> Unit) {
    WalletCard {
        (0 until count step 2).forEachIndexed { row, first ->
            if (row > 0) CardDivider()
            Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
                SeedCell(first, cell, Modifier.weight(1f))
                VerticalDivider(thickness = 1.dp, color = MaterialTheme.colorScheme.outline)
                if (first + 1 < count) SeedCell(first + 1, cell, Modifier.weight(1f)) else Box(Modifier.weight(1f))
            }
        }
    }
}

@Composable
private fun SeedCell(index: Int, cell: @Composable RowScope.(Int) -> Unit, modifier: Modifier) {
    Row(
        modifier.padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(14.dp),
    ) {
        Text(
            "%02d".format(index + 1),
            style = WalletText.mono,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.width(20.dp),
        )
        cell(index)
    }
}

/** A word shown in a [SeedGrid] cell. */
@Composable
fun SeedWord(word: String) = Text(word, style = MaterialTheme.typography.titleMedium)
