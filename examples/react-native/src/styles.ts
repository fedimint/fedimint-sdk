import { StyleSheet, Platform } from 'react-native'

// Claymorphism uses soft pastel or off-white backgrounds with diffuse shadows
// to create a 3D, tactile "clay-like" appearance.
const BG_COLOR = '#E0E5EC' // A soft, cool off-white
const CLAY_LITE = '#ffffff'
const CLAY_DARK = '#a3b1c6'
const TEXT_DARK = '#2d3748'
const TEXT_MUTED = '#718096'

// Reusable shadow styles for the "floating clay" effect
const clayShadows = {
  ...Platform.select({
    ios: {
      shadowColor: CLAY_DARK,
      shadowOffset: { width: 6, height: 6 },
      shadowOpacity: 0.8,
      shadowRadius: 10,
    },
    android: {
      elevation: 8,
    },
  })
}

const s = StyleSheet.create({
  // Main wrapping area
  safeArea: {
    flex: 1,
    backgroundColor: BG_COLOR,
  },
  container: {
    flex: 1,
    backgroundColor: BG_COLOR,
  },
  contentContainer: {
    padding: 20,
    paddingBottom: 60,
  },
  // Typography
  header: {
    fontSize: 28,
    fontWeight: '800',
    color: TEXT_DARK,
    textAlign: 'center',
    marginBottom: 24,
    letterSpacing: 0.5,
  },
  // Soft, padded sections that feel like lifted clay blocks
  section: {
    backgroundColor: BG_COLOR,
    borderRadius: 24,
    padding: 24,
    marginBottom: 20,
    ...clayShadows,
  },
  sectionTitle: {
    color: TEXT_DARK,
    fontSize: 20,
    fontWeight: '800',
    marginBottom: 16,
    letterSpacing: 0.5,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    flexWrap: 'wrap',
    gap: 12,
    marginBottom: 12,
  },
  label: {
    color: TEXT_MUTED,
    fontSize: 16,
    fontWeight: '600',
    marginBottom: 4,
  },
  value: {
    color: TEXT_DARK,
    fontSize: 16,
    fontWeight: '500',
  },
  balance: {
    color: '#4fd1c5', // Soft teal
    fontSize: 28,
    fontWeight: '900',
  },
  mono: {
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
    color: '#6b46c1', // Soft purple
    fontSize: 13,
  },
  italic: {
    color: TEXT_MUTED,
    fontStyle: 'italic',
    marginTop: 8,
    lineHeight: 20,
  },
  link: {
    color: '#4299e1', // Soft bright blue
    textDecorationLine: 'underline',
    marginTop: 4,
    fontSize: 14,
  },
  // Neumorphic/clay inputs 
  input: {
    backgroundColor: '#ebf0f5', // Slightly darker than background for inset feel
    color: TEXT_DARK,
    borderRadius: 16,
    paddingHorizontal: 16,
    paddingVertical: 14,
    fontSize: 16,
    marginBottom: 12,
    // Add inner shadow or subtle border to simulate inset
    borderWidth: 1,
    borderColor: '#d1d8e0',
  },
  textArea: {
    backgroundColor: '#ebf0f5',
    color: TEXT_DARK,
    borderRadius: 16,
    paddingHorizontal: 16,
    paddingVertical: 14,
    fontSize: 16,
    marginBottom: 12,
    borderWidth: 1,
    borderColor: '#d1d8e0',
    minHeight: 80,
    textAlignVertical: 'top',
  },
  formGroup: {
    marginTop: 12,
  },
  // Big soft buttons
  btn: {
    backgroundColor: BG_COLOR,
    paddingHorizontal: 20,
    paddingVertical: 14,
    borderRadius: 16,
    alignItems: 'center',
    justifyContent: 'center',
    ...clayShadows,
  },
  btnActive: {
    backgroundColor: '#cbd5e0', // Slightly pressed state
  },
  btnSmall: {
    paddingHorizontal: 16,
    paddingVertical: 8,
    borderRadius: 12,
  },
  btnPrimary: {
    backgroundColor: '#4fd1c5', // Soft teal primary button
  },
  btnDisabled: {
    opacity: 0.5,
  },
  btnText: {
    color: TEXT_DARK,
    fontSize: 16,
    fontWeight: '700',
    letterSpacing: 0.5,
  },
  btnTextSmall: {
    fontSize: 14,
  },
  btnTextDisabled: {
    color: TEXT_MUTED,
  },
  // Display blocks
  mnemonicDisplay: {
    backgroundColor: '#ebf0f5',
    borderRadius: 16,
    padding: 16,
    marginTop: 12,
    borderWidth: 1,
    borderColor: '#d1d8e0',
  },
  mnemonicText: {
    color: TEXT_DARK,
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
    fontSize: 15,
    lineHeight: 24,
    letterSpacing: 1,
  },
  mnemonicBlurred: {
    color: TEXT_DARK,
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
    fontSize: 15,
    lineHeight: 24,
    opacity: 0.1,
  },
  // Alert boxes mapped to clay concept
  success: {
    backgroundColor: '#F0FFF4', // Soft mint
    borderRadius: 16,
    padding: 16,
    marginTop: 12,
    borderLeftWidth: 4,
    borderColor: '#48BB78',
  },
  successText: {
    color: '#276749',
    fontSize: 15,
    fontWeight: '500',
  },
  error: {
    backgroundColor: '#FFF5F5', // Soft blush
    borderRadius: 16,
    padding: 16,
    marginTop: 12,
    borderLeftWidth: 4,
    borderColor: '#F56565',
  },
  errorText: {
    color: '#9B2C2C',
    fontSize: 15,
    fontWeight: '500',
  },
  // Misc cards
  previewCard: {
    backgroundColor: BG_COLOR,
    borderRadius: 16,
    padding: 16,
    marginTop: 16,
    ...clayShadows,
  },
  previewTitle: {
    color: TEXT_DARK,
    fontSize: 18,
    fontWeight: '800',
    marginBottom: 12,
  },
  guardianItem: {
    marginLeft: 8,
    marginBottom: 8,
  },
  guardianName: {
    color: TEXT_DARK,
    fontWeight: '700',
    fontSize: 15,
  },
  guardianUrl: {
    color: '#4299e1',
    fontSize: 13,
    fontFamily: Platform.OS === 'ios' ? 'Menlo' : 'monospace',
  },
  invoiceBox: {
    backgroundColor: '#ebf0f5',
    borderRadius: 16,
    padding: 16,
    marginTop: 12,
    borderWidth: 1,
    borderColor: '#d1d8e0',
  },
  resultBox: {
    backgroundColor: '#ebf0f5',
    borderRadius: 16,
    padding: 16,
    marginTop: 12,
    borderWidth: 1,
    borderColor: '#d1d8e0',
  },
  stepsCard: {
    backgroundColor: BG_COLOR,
    borderRadius: 24,
    padding: 24,
    marginBottom: 20,
    ...clayShadows,
  },
  stepsTitle: {
    color: TEXT_DARK,
    fontWeight: '800',
    fontSize: 16,
    marginBottom: 12,
  },
  stepItem: {
    color: TEXT_MUTED,
    fontSize: 15,
    marginBottom: 8,
    paddingLeft: 8,
    lineHeight: 22,
  },
})

export default s