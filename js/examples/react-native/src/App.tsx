import React, { useCallback, useEffect, useState } from 'react'
import {
  ActivityIndicator,
  BackHandler,
  LogBox,
  Platform,
  PlatformColor,
  Pressable,
  StatusBar,
  StyleSheet,
  Text,
  View,
} from 'react-native'
import {
  DefaultTheme,
  NavigationContainer,
  useNavigation,
} from '@react-navigation/native'
import {
  createNativeStackNavigator,
  type NativeStackNavigationProp,
} from '@react-navigation/native-stack'
import {
  createBottomTabNavigator,
  type BottomTabNavigationProp,
} from '@react-navigation/bottom-tabs'
import { createNativeBottomTabNavigator } from '@react-navigation/bottom-tabs/unstable'
import type { ActivityItem } from '@fedimint/react-native'
import { SafeAreaProvider } from 'react-native-safe-area-context'
import { Icon } from './components/Icon'
import { model, useDemo } from './model/DemoModel'
import { FedimintLogoOnDark } from './components/Brand'
import { WelcomeScreen } from './screens/onboarding/WelcomeScreen'
import { RestoreScreen } from './screens/onboarding/RestoreScreen'
import { BackupScreen } from './screens/onboarding/BackupScreen'
import { VerifyScreen } from './screens/onboarding/VerifyScreen'
import { HomeScreen } from './screens/home/HomeScreen'
import { ActivityScreen } from './screens/activity/ActivityScreen'
import { OperationDetailScreen } from './screens/activity/OperationDetailScreen'
import { ActivityText } from './components/Activity'
import { FederationsScreen } from './screens/federations/FederationsScreen'
import { JoinScreen } from './screens/federations/JoinScreen'
import { FederationDetailScreen } from './screens/federations/FederationDetailScreen'
import { RecoveryScreen } from './screens/federations/RecoveryScreen'
import { ReceiveScreen } from './screens/receive/ReceiveScreen'
import { SendScreen } from './screens/send/SendScreen'
import { ReviewScreen } from './screens/send/ReviewScreen'
import type { PaymentRail } from './components/Status'
import { Metrics, Palette, Typeface } from './theme/tokens'
import { Button } from './components/Buttons'
import { reason } from './model/errors'

// An operation's screen is handed its activity item, whose amounts and time are BigInt. React
// Navigation warns that such params cannot be serialised, which matters only to an app that
// persists its navigation state or opens from links; this one does neither.
LogBox.ignoreLogs([
  'Non-serializable values were found in the navigation state',
])

const theme = {
  ...DefaultTheme,
  colors: {
    ...DefaultTheme.colors,
    primary: Palette.ink,
    background: Palette.background,
    card: Palette.background,
    text: Palette.ink,
    border: Palette.border,
  },
}

/** The bars the iOS demo uses: cream, an inline title, no hairline. */
const stackOptions = {
  headerStyle: { backgroundColor: Palette.background },
  headerShadowVisible: false,
  headerTitleStyle: {
    fontFamily: 'Geist-SemiBold',
    fontSize: 17,
    color: Palette.ink,
  },
  headerTintColor: Palette.ink,
  headerBackButtonDisplayMode: 'minimal' as const,
  // Centred, as iOS sets every inline title; Android's own default is beside the back button.
  headerTitleAlign: 'center' as const,
}

type OnboardingRoutes = { Welcome: undefined; Restore: undefined }
const Onboarding = createNativeStackNavigator<OnboardingRoutes>()

/** Everything before a wallet is open: Welcome, and Restore pushed from it. */
function OnboardingStack() {
  return (
    <Onboarding.Navigator
      screenOptions={stackOptions}
      screenListeners={({ route }) => ({
        // A failure from the screen just left is about that screen, not the one returned to. On
        // leaving Restore, not on Welcome's focus: its first focus follows a reopen that failed,
        // whose reason is shown there.
        beforeRemove: () => {
          if (route.name === 'Restore' && !model.isAttaching) {
            model.clearFailure('wallet')
          }
        },
      })}
    >
      <Onboarding.Screen name="Welcome" options={{ headerShown: false }}>
        {({ navigation }) => (
          <WelcomeScreen restore={() => navigation.navigate('Restore')} />
        )}
      </Onboarding.Screen>
      <Onboarding.Screen
        name="Restore"
        component={RestoreScreen}
        options={{ title: 'Restore' }}
      />
    </Onboarding.Navigator>
  )
}

type BackupRoutes = { Backup: undefined; Verify: undefined }
const Backup = createNativeStackNavigator<BackupRoutes>()

/** Back up, then Verify, straight after a wallet is created and before it is used. */
function BackupFlow({ done }: { done: () => void }) {
  const state = useDemo()
  // Hides the phrase on every way out: finishing, and on Android the back button, which closes
  // the app while the model, and so the words, stay in memory for its next launch.
  useEffect(() => () => model.hideSeed(), [])
  return (
    <Backup.Navigator screenOptions={{ ...stackOptions, title: '' }}>
      <Backup.Screen name="Backup" options={{ headerBackVisible: false }}>
        {({ navigation }) => (
          <BackupScreen
            next={() => navigation.navigate('Verify')}
            later={done}
          />
        )}
      </Backup.Screen>
      <Backup.Screen name="Verify">
        {() => <VerifyScreen words={state.seedWords} finish={done} />}
      </Backup.Screen>
    </Backup.Navigator>
  )
}

type MainRoutes = {
  Tabs: undefined
  Send: undefined
  Receive: undefined
  RecoveryPhrase: undefined
}
const Main = createNativeStackNavigator<MainRoutes>()

type HomeRoutes = {
  HomeRoot: undefined
  Operation: { item: ActivityItem }
  Recovery: undefined
  FederationDetail: undefined
}
const HomeStack = createNativeStackNavigator<HomeRoutes>()

type ActivityRoutes = {
  ActivityRoot: undefined
  Operation: { item: ActivityItem }
}

type FederationsRoutes = {
  FederationsRoot: undefined
  Join: undefined
  FederationDetail: undefined
  Recovery: undefined
}
const FederationsStack = createNativeStackNavigator<FederationsRoutes>()
const ActivityStack = createNativeStackNavigator<ActivityRoutes>()

type TabRoutes = {
  Home: undefined
  Activity: undefined
  Federations: undefined
}
const Tab = createBottomTabNavigator<TabRoutes>()
// iOS uses the system tab bar, as the Swift demo's TabView does: from iOS 26 that is the floating
// Liquid Glass bar, which a tab bar drawn in JS does not match.
// The icons are the filled symbols SwiftUI's tab bar picks for the names the Swift demo gives.
const NativeTab = createNativeBottomTabNavigator<TabRoutes>()

function OperationRoute({
  route,
  navigation,
}: {
  route: { params: { item: ActivityItem } }
  navigation: { goBack: () => void; canGoBack: () => boolean }
}) {
  const close = useCallback(() => {
    if (navigation.canGoBack()) navigation.goBack()
  }, [navigation])
  return <OperationDetailScreen item={route.params.item} close={close} />
}

function operationTitle({
  route,
}: {
  route: { params: { item: ActivityItem } }
}) {
  return { title: ActivityText.rail(route.params.item.kind) }
}

function HomeTab() {
  const main = useNavigation<NativeStackNavigationProp<MainRoutes>>()
  const tabs = useNavigation<BottomTabNavigationProp<TabRoutes>>()
  return (
    <HomeStack.Navigator screenOptions={stackOptions}>
      {/* Home's header row stands in for a navigation bar. */}
      <HomeStack.Screen name="HomeRoot" options={{ headerShown: false }}>
        {({ navigation }) => (
          <HomeScreen
            receive={() => main.navigate('Receive')}
            send={() => main.navigate('Send')}
            showActivity={() => tabs.navigate('Activity')}
            showFederations={() => tabs.navigate('Federations')}
            showFederationDetails={() =>
              navigation.navigate('FederationDetail')
            }
            showRecovery={() => navigation.navigate('Recovery')}
            showRecoveryPhrase={() => main.navigate('RecoveryPhrase')}
            showOperation={(item) => navigation.navigate('Operation', { item })}
          />
        )}
      </HomeStack.Screen>
      <HomeStack.Screen
        name="Operation"
        component={OperationRoute}
        options={operationTitle}
      />
      <HomeStack.Screen
        name="Recovery"
        component={RecoveryRoute}
        options={{ title: 'Recovery' }}
      />
      <HomeStack.Screen
        name="FederationDetail"
        options={{ title: 'Federation' }}
      >
        {({ navigation }) => (
          <FederationDetailScreen
            showRecovery={() => navigation.navigate('Recovery')}
          />
        )}
      </HomeStack.Screen>
    </HomeStack.Navigator>
  )
}

function ActivityTab() {
  return (
    <ActivityStack.Navigator screenOptions={stackOptions}>
      <ActivityStack.Screen name="ActivityRoot" options={{ title: 'Activity' }}>
        {({ navigation }) => (
          <ActivityScreen
            showOperation={(item) => navigation.navigate('Operation', { item })}
          />
        )}
      </ActivityStack.Screen>
      <ActivityStack.Screen
        name="Operation"
        component={OperationRoute}
        options={operationTitle}
      />
    </ActivityStack.Navigator>
  )
}

function RecoveryRoute({
  navigation,
}: {
  navigation: { goBack: () => void; canGoBack: () => boolean }
}) {
  const close = useCallback(() => {
    if (navigation.canGoBack()) navigation.goBack()
  }, [navigation])
  return <RecoveryScreen close={close} />
}

function FederationsTab() {
  return (
    <FederationsStack.Navigator screenOptions={stackOptions}>
      <FederationsStack.Screen
        name="FederationsRoot"
        options={({ navigation }) => ({
          title: 'Federations',
          ...(Platform.OS === 'ios'
            ? {
                unstable_headerRightItems: () => [
                  {
                    type: 'button' as const,
                    label: 'Join a federation',
                    icon: { type: 'sfSymbol' as const, name: 'plus' },
                    onPress: () => navigation.navigate('Join'),
                  },
                ],
              }
            : {
                headerRight: () => (
                  <Pressable
                    accessibilityLabel="Join a federation"
                    hitSlop={8}
                    onPress={() => navigation.navigate('Join')}
                  >
                    <Icon name="plus" size={20} color={Palette.ink} />
                  </Pressable>
                ),
              }),
        })}
      >
        {({ navigation }) => (
          <FederationsScreen
            join={() => navigation.navigate('Join')}
            detail={() => navigation.navigate('FederationDetail')}
          />
        )}
      </FederationsStack.Screen>
      <FederationsStack.Screen name="Join" options={{ title: 'Join' }}>
        {({ navigation }) => <JoinScreen close={navigation.goBack} />}
      </FederationsStack.Screen>
      <FederationsStack.Screen
        name="FederationDetail"
        options={{ title: 'Federation' }}
      >
        {({ navigation }) => (
          <FederationDetailScreen
            showRecovery={() => navigation.navigate('Recovery')}
          />
        )}
      </FederationsStack.Screen>
      <FederationsStack.Screen
        name="Recovery"
        component={RecoveryRoute}
        options={{ title: 'Recovery' }}
      />
    </FederationsStack.Navigator>
  )
}

function Tabs() {
  if (Platform.OS === 'ios') {
    return (
      <NativeTab.Navigator
        screenOptions={{
          headerShown: false,
          tabBarActiveTintColor: Palette.primary,
          // UIKit's own unselected color: gray before iOS 26, the label color on the glass bar.
          ...(parseInt(String(Platform.Version), 10) < 26 && {
            tabBarInactiveTintColor: PlatformColor('systemGray'),
          }),
        }}
      >
        <NativeTab.Screen
          name="Home"
          component={HomeTab}
          options={{ tabBarIcon: { type: 'sfSymbol', name: 'house.fill' } }}
        />
        <NativeTab.Screen
          name="Activity"
          component={ActivityTab}
          options={{
            tabBarIcon: { type: 'sfSymbol', name: 'waveform.path.ecg' },
          }}
        />
        <NativeTab.Screen
          name="Federations"
          component={FederationsTab}
          options={{
            tabBarIcon: { type: 'sfSymbol', name: 'building.columns.fill' },
          }}
        />
      </NativeTab.Navigator>
    )
  }
  return (
    <Tab.Navigator
      screenOptions={{
        headerShown: false,
        tabBarActiveTintColor: Palette.primary,
        tabBarInactiveTintColor: Palette.muted,
        tabBarLabelStyle: Typeface.geist('Medium', 11),
        tabBarStyle: { backgroundColor: Palette.surface },
      }}
    >
      <Tab.Screen
        name="Home"
        component={HomeTab}
        options={{
          tabBarIcon: ({ color }) => (
            <Icon name="house" size={22} color={color} />
          ),
        }}
      />
      <Tab.Screen
        name="Activity"
        component={ActivityTab}
        options={{
          tabBarIcon: ({ color }) => (
            <Icon name="waveform.path.ecg" size={22} color={color} />
          ),
        }}
      />
      <Tab.Screen
        name="Federations"
        component={FederationsTab}
        options={{
          tabBarIcon: ({ color }) => (
            <Icon name="building.columns" size={22} color={color} />
          ),
        }}
      />
    </Tab.Navigator>
  )
}

type SendRoutes = { SendForm: undefined; Review: { rail: PaymentRail } }
const SendStack = createNativeStackNavigator<SendRoutes>()

/**
 * "Close" on a sheet's leading edge, as the iOS demo's cancellation action. On iOS a native bar
 * button, so it takes the system's own look, the glass capsule on iOS 26 and later, exactly as the
 * SwiftUI toolbar button does; Android has no such item and draws the text.
 */
function closeButton(close: () => void) {
  return Platform.OS === 'ios'
    ? {
        unstable_headerLeftItems: () => [
          { type: 'button' as const, label: 'Close', onPress: close },
        ],
      }
    : {
        headerLeft: () => (
          <Pressable onPress={close} hitSlop={8}>
            <Text style={[Typeface.body, { color: Palette.ink }]}>Close</Text>
          </Pressable>
        ),
      }
}

/** The review, which becomes the result once the payment is sent. */
function ReviewRoute({
  route,
  navigation,
  done,
}: {
  route: { params: { rail: PaymentRail } }
  navigation: NativeStackNavigationProp<SendRoutes, 'Review'>
  done: () => void
}) {
  const [submitted, setSubmitted] = useState(false)
  const { rail } = route.params
  useEffect(() => {
    navigation.setOptions({
      title: submitted
        ? rail === 'ecash'
          ? 'Ecash notes'
          : ''
        : 'Review payment',
      // Once sent there is no going back to the quote: it is spent.
      headerBackVisible: !submitted,
      gestureEnabled: !submitted,
    })
  }, [submitted, rail, navigation])
  // Android's back button is not covered by the two options above: once sent it closes the
  // sheet, as swiping it down does on iOS, rather than reopening the spent quote's form.
  useEffect(() => {
    if (!submitted) return
    const subscription = BackHandler.addEventListener(
      'hardwareBackPress',
      () => {
        done()
        return true
      },
    )
    return () => subscription.remove()
  }, [submitted, done])
  return (
    <ReviewScreen
      rail={rail}
      done={done}
      back={navigation.goBack}
      submitted={submitted}
      setSubmitted={() => setSubmitted(true)}
    />
  )
}

/** 14 Send, 06 Review and the result, in their own stack inside the Send sheet. */
function SendFlow({ navigation }: { navigation: { goBack: () => void } }) {
  const close = navigation.goBack
  return (
    <SendStack.Navigator screenOptions={stackOptions}>
      <SendStack.Screen
        name="SendForm"
        options={{ title: 'Send', ...closeButton(close) }}
      >
        {({ navigation: stack }) => (
          <SendScreen
            review={(rail) => stack.navigate('Review', { rail })}
            close={close}
          />
        )}
      </SendStack.Screen>
      <SendStack.Screen name="Review" options={{ title: 'Review payment' }}>
        {(props) => <ReviewRoute {...props} done={close} />}
      </SendStack.Screen>
    </SendStack.Navigator>
  )
}

/** The recovery phrase again, later: the backup screen from the wallet's menu. */
function RecoveryPhraseSheet({
  navigation,
}: {
  navigation: { goBack: () => void }
}) {
  // Every way off the sheet, Close and swiping it away alike, so the phrase is never still
  // revealed the next time it opens.
  useEffect(() => () => model.hideSeed(), [])
  return (
    <BackupScreen
      next={navigation.goBack}
      later={navigation.goBack}
      step={null}
    />
  )
}

/** The wallet once one is open: the design's three tabs, with Send and Receive as sheets. */
function MainTabs() {
  return (
    <Main.Navigator screenOptions={stackOptions}>
      <Main.Screen
        name="Tabs"
        component={Tabs}
        options={{ headerShown: false }}
      />
      <Main.Group screenOptions={{ presentation: 'modal' }}>
        <Main.Screen
          name="Send"
          component={SendFlow}
          options={{ headerShown: false }}
        />
        <Main.Screen
          name="Receive"
          component={ReceiveScreen}
          options={({ navigation }) => ({
            title: 'Receive',
            ...closeButton(navigation.goBack),
          })}
        />
        <Main.Screen
          name="RecoveryPhrase"
          component={RecoveryPhraseSheet}
          options={({ navigation }) => ({
            title: '',
            ...closeButton(navigation.goBack),
          })}
        />
      </Main.Group>
    </Main.Navigator>
  )
}

/**
 * Shown while a wallet left by an earlier launch is reopened, so the first frame is not a
 * Welcome screen offering to create a wallet that already exists.
 */
function LaunchScreen({
  failure,
  retry,
}: {
  failure?: string
  retry: () => void
}) {
  return (
    <View style={styles.launch}>
      <FedimintLogoOnDark height={32} />
      {failure === undefined ? (
        <ActivityIndicator color="#FFFFFF" />
      ) : (
        <View style={styles.launchFailure}>
          <Text
            selectable
            style={[
              Typeface.callout,
              { color: '#FFFFFF', textAlign: 'center' },
            ]}
          >
            Couldn't move the wallet out of Documents: {failure}
          </Text>
          <Button title="Try again" kind="primaryOnDark" onPress={retry} />
        </View>
      )}
    </View>
  )
}

/**
 * The root: picks the stage the app is in, as the iOS demo's ContentView does. Reopening a
 * stored wallet, onboarding, the backup step right after creation, or the wallet itself.
 */
export default function App() {
  const state = useDemo()
  /** Whether an earlier launch left a wallet behind, read once at launch. */
  const [reopening, setReopening] = useState<boolean>()
  /** Why the launch stopped before onboarding: the old wallet could not be moved. */
  const [launchFailure, setLaunchFailure] = useState<string>()
  const attaching = model.isAttaching

  const launch = useCallback(() => {
    setLaunchFailure(undefined)
    model.start().then(
      (stored) => {
        setReopening(stored)
        if (stored) model.openWallet()
      },
      (error) => setLaunchFailure(reason(error)),
    )
  }, [])

  useEffect(() => {
    // On Android the root remounts after the back button closes the app, while the JS runtime,
    // and the wallet it opened, carry on. That wallet is still open, and opening it again would
    // be refused as storage in use.
    if (model.getState().hasSdk) return
    launch()
  }, [launch])

  useEffect(() => {
    // A reopen that failed falls back to onboarding, where its reason is shown.
    if (!attaching && !state.hasSdk && reopening) setReopening(false)
  }, [attaching, state.hasSdk, reopening])

  let stage: React.ReactNode
  let dark = false
  if (state.hasSdk && state.walletOrigin === 'created' && !state.backedUp) {
    stage = <BackupFlow done={() => model.markBackedUp()} />
  } else if (state.hasSdk) {
    stage = <MainTabs />
  } else if (reopening !== false) {
    stage = <LaunchScreen failure={launchFailure} retry={launch} />
    dark = true
  } else {
    stage = <OnboardingStack />
    dark = true
  }

  return (
    <SafeAreaProvider>
      <StatusBar barStyle={dark ? 'light-content' : 'dark-content'} />
      <NavigationContainer theme={theme}>{stage}</NavigationContainer>
    </SafeAreaProvider>
  )
}

const styles = StyleSheet.create({
  launch: {
    flex: 1,
    backgroundColor: Palette.navy,
    alignItems: 'center',
    justifyContent: 'center',
    gap: Metrics.sectionGap,
  },
  launchFailure: {
    alignSelf: 'stretch',
    gap: Metrics.gap,
    paddingHorizontal: Metrics.screenPadding,
  },
})
