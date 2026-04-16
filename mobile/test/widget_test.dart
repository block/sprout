import 'package:flutter_test/flutter_test.dart';
import 'package:hooks_riverpod/hooks_riverpod.dart';
import 'package:shared_preferences/shared_preferences.dart';
import 'package:sprout_mobile/app.dart';
import 'package:sprout_mobile/shared/auth/auth.dart';
import 'package:sprout_mobile/shared/theme/theme_provider.dart';

void main() {
  testWidgets('App renders pairing page when unauthenticated', (
    WidgetTester tester,
  ) async {
    SharedPreferences.setMockInitialValues({});
    final prefs = await SharedPreferences.getInstance();

    await tester.pumpWidget(
      ProviderScope(
        overrides: [
          authProvider.overrideWith(() => _FakeAuthNotifier()),
          savedPrefsProvider.overrideWithValue(prefs),
        ],
        child: const App(),
      ),
    );
    await tester.pump();
    expect(find.text('Welcome to Sprout'), findsOneWidget);
  });
}

class _FakeAuthNotifier extends AuthNotifier {
  @override
  Future<AuthState> build() async {
    return const AuthState(status: AuthStatus.unauthenticated);
  }
}
