import 'package:flutter_test/flutter_test.dart';
import 'package:sprout_mobile/app.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';

void main() {
  testWidgets('App renders without crashing', (WidgetTester tester) async {
    await tester.pumpWidget(const ProviderScope(child: App()));
    expect(find.text('Sprout'), findsWidgets);
  });
}
