import { StyleSheet } from 'react-native'

const s = StyleSheet.create({
  safeArea: {
    flex: 1,
    backgroundColor: '#1a1a2e',
  },
  container: {
    flex: 1,
    backgroundColor: '#1a1a2e',
  },
  contentContainer: {
    padding: 16,
    paddingBottom: 40,
  },
  header: {
    fontSize: 24,
    fontWeight: 'bold',
    color: '#ffffff',
    textAlign: 'center',
    marginBottom: 16,
  },
  section: {
    backgroundColor: '#16213e',
    borderRadius: 12,
    padding: 16,
    marginBottom: 12,
  },
  sectionTitle: {
    color: '#ffffff',
    fontSize: 18,
    fontWeight: '700',
    marginBottom: 12,
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    flexWrap: 'wrap',
    gap: 8,
    marginBottom: 8,
  },
  label: {
    color: '#b0b0b0',
    fontSize: 14,
    fontWeight: '600',
    marginBottom: 4,
  },
  value: {
    color: '#e0e0e0',
    fontSize: 14,
  },
  balance: {
    color: '#4ade80',
    fontSize: 20,
    fontWeight: 'bold',
  },
  balanceLarge: {
    color: '#4ade80',
    fontSize: 36,
    fontWeight: 'bold',
    textAlign: 'center',
    marginVertical: 12,
  },
  balanceLabel: {
    color: '#b0b0b0',
    fontSize: 14,
    textAlign: 'center',
  },
  mono: {
    fontFamily: 'monospace',
    color: '#93c5fd',
    fontSize: 12,
  },
  italic: {
    color: '#888',
    fontStyle: 'italic',
    marginTop: 8,
  },
  link: {
    color: '#60a5fa',
    textDecorationLine: 'underline',
    marginTop: 4,
    fontSize: 13,
  },
  input: {
    backgroundColor: '#0f3460',
    color: '#ffffff',
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 14,
    marginBottom: 8,
    borderWidth: 1,
    borderColor: '#1a4a7a',
  },
  textArea: {
    backgroundColor: '#0f3460',
    color: '#ffffff',
    borderRadius: 8,
    paddingHorizontal: 12,
    paddingVertical: 10,
    fontSize: 14,
    marginBottom: 8,
    borderWidth: 1,
    borderColor: '#1a4a7a',
    minHeight: 60,
    textAlignVertical: 'top',
  },
  formGroup: {
    marginTop: 8,
  },
  btn: {
    backgroundColor: '#0f3460',
    paddingHorizontal: 16,
    paddingVertical: 10,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
  },
  btnActive: {
    backgroundColor: '#1a5276',
    borderWidth: 1,
    borderColor: '#60a5fa',
  },
  btnSmall: {
    paddingHorizontal: 12,
    paddingVertical: 6,
  },
  btnPrimary: {
    backgroundColor: '#4CAF50',
  },
  btnDisabled: {
    opacity: 0.5,
  },
  btnText: {
    color: '#ffffff',
    fontSize: 14,
    fontWeight: '600',
  },
  btnTextSmall: {
    fontSize: 12,
  },
  btnTextDisabled: {
    color: '#aaa',
  },
  actionRow: {
    flexDirection: 'row',
    gap: 12,
    marginTop: 16,
  },
  actionBtn: {
    flex: 1,
    backgroundColor: '#0f3460',
    paddingVertical: 14,
    borderRadius: 12,
    alignItems: 'center',
  },
  actionBtnText: {
    color: '#60a5fa',
    fontSize: 16,
    fontWeight: '700',
  },
  mnemonicDisplay: {
    backgroundColor: '#0f3460',
    borderRadius: 8,
    padding: 12,
    marginTop: 8,
  },
  mnemonicText: {
    color: '#e0e0e0',
    fontFamily: 'monospace',
    fontSize: 14,
    lineHeight: 22,
    marginBottom: 8,
  },
  mnemonicBlurred: {
    color: '#e0e0e0',
    fontFamily: 'monospace',
    fontSize: 14,
    lineHeight: 22,
    marginBottom: 8,
    opacity: 0.1,
  },
  success: {
    backgroundColor: '#064e3b',
    borderRadius: 8,
    padding: 10,
    marginTop: 8,
  },
  successText: {
    color: '#6ee7b7',
    fontSize: 13,
  },
  error: {
    backgroundColor: '#7f1d1d',
    borderRadius: 8,
    padding: 10,
    marginTop: 8,
  },
  errorText: {
    color: '#fca5a5',
    fontSize: 13,
  },
  previewCard: {
    backgroundColor: '#0f3460',
    borderRadius: 8,
    padding: 12,
    marginTop: 12,
  },
  previewTitle: {
    color: '#ffffff',
    fontSize: 16,
    fontWeight: '700',
    marginBottom: 8,
  },
  guardianItem: {
    marginLeft: 8,
    marginBottom: 4,
  },
  guardianName: {
    color: '#e0e0e0',
    fontWeight: '600',
    fontSize: 13,
  },
  guardianUrl: {
    color: '#93c5fd',
    fontSize: 12,
    fontFamily: 'monospace',
  },
  invoiceBox: {
    backgroundColor: '#0f3460',
    borderRadius: 8,
    padding: 12,
    marginTop: 8,
  },
  resultBox: {
    backgroundColor: '#0f3460',
    borderRadius: 8,
    padding: 12,
    marginTop: 8,
  },
  txItem: {
    backgroundColor: '#0f3460',
    borderRadius: 8,
    padding: 12,
    marginBottom: 8,
  },
  txType: {
    fontSize: 14,
    fontWeight: '700',
  },
  txIncoming: {
    color: '#4ade80',
  },
  txOutgoing: {
    color: '#f87171',
  },
  txAmount: {
    color: '#e0e0e0',
    fontSize: 16,
    fontWeight: '600',
    marginTop: 4,
  },
  txDate: {
    color: '#888',
    fontSize: 12,
    marginTop: 2,
  },
  emptyText: {
    color: '#888',
    fontSize: 14,
    textAlign: 'center',
    marginTop: 24,
  },
})

export default s
