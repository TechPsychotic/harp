import 'package:flutter/material.dart';
import 'package:flutter/services.dart';

import '../player/controller.dart';
import '../theme/glass.dart';
import 'song_actions.dart';

/// The player's error, tappable: shows what actually went wrong, to copy into
/// a bug report.
class PlayerErrorText extends StatelessWidget {
  const PlayerErrorText({super.key, required this.player, this.center = false, this.maxLines = 3});
  final PlayerController player;
  final bool center;
  final int maxLines;

  @override
  Widget build(BuildContext context) {
    final e = player.error;
    if (e == null) return const SizedBox.shrink();
    return InkWell(
      borderRadius: BorderRadius.circular(8),
      onTap: () => showErrorDetail(context, player),
      child: Padding(
        padding: const EdgeInsets.symmetric(vertical: 2),
        child: Text.rich(
          TextSpan(children: [
            TextSpan(text: e),
            const TextSpan(text: '  Details', style: TextStyle(decoration: TextDecoration.underline, fontWeight: FontWeight.w600)),
          ]),
          textAlign: center ? TextAlign.center : TextAlign.start,
          maxLines: maxLines,
          overflow: TextOverflow.ellipsis,
          style: const TextStyle(color: Color(0xFFFF8A8A), fontSize: 13),
        ),
      ),
    );
  }
}

Future<void> showErrorDetail(BuildContext context, PlayerController player) {
  final detail = player.errorDetail ?? player.error ?? '';
  return showModalBottomSheet(
    context: context,
    showDragHandle: true,
    builder: (context) => SafeArea(
      child: Padding(
        padding: const EdgeInsets.fromLTRB(20, 0, 20, 16),
        child: Column(mainAxisSize: MainAxisSize.min, crossAxisAlignment: CrossAxisAlignment.start, children: [
          const Text('What went wrong', style: TextStyle(fontWeight: FontWeight.w700, fontSize: 17)),
          const SizedBox(height: 10),
          ConstrainedBox(
            constraints: const BoxConstraints(maxHeight: 260),
            child: SingleChildScrollView(child: SelectableText(detail, style: TextStyle(color: context.tok.textDim, fontSize: 13, fontFamily: 'monospace'))),
          ),
          const SizedBox(height: 12),
          Row(children: [
            FilledButton.icon(
              onPressed: () async {
                await Clipboard.setData(ClipboardData(text: detail));
                if (context.mounted) {
                  Navigator.pop(context);
                  toast(context, 'Copied');
                }
              },
              icon: const Icon(Icons.copy_rounded),
              label: const Text('Copy'),
            ),
            const SizedBox(width: 10),
            TextButton(onPressed: () => Navigator.pop(context), child: const Text('Close')),
          ]),
        ]),
      ),
    ),
  );
}
