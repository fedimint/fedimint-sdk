import * as React from 'react';

import { StyleSheet, View, Text, TouchableOpacity } from 'react-native';
import WalletDirector from '@fedimint/react-native';
import RNFS from 'react-native-fs';

let director: WalletDirector;

const initFedimintCLI = async () => {
  const dbPath = `${RNFS.DocumentDirectoryPath}/fedimint_db`;

  console.log('Using DB Path:', dbPath);

  director = new WalletDirector(dbPath);
};

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#1a1a1a',
    padding: 20,
  },
  content: {
    flex: 1,
    justifyContent: 'center',
    alignItems: 'center',
  },
  header: {
    fontSize: 28,
    fontWeight: 'bold',
    color: '#ffffff',
    marginBottom: 40,
    letterSpacing: 0.5,
  },
  card: {
    backgroundColor: '#2d2d2d',
    borderRadius: 16,
    padding: 24,
    width: '100%',
    marginBottom: 30,
    shadowColor: '#000',
    shadowOffset: {
      width: 0,
      height: 4,
    },
    shadowOpacity: 0.3,
    shadowRadius: 4.65,
    elevation: 8,
  },
  mnemonicLabel: {
    color: '#888888',
    fontSize: 14,
    marginBottom: 12,
    textTransform: 'uppercase',
    letterSpacing: 1,
  },
  mnemonicText: {
    color: '#ffffff',
    fontSize: 18,
    lineHeight: 28,
    textAlign: 'center',
    fontFamily: 'Courier New',
  },
  placeholderText: {
    color: '#555555',
    fontSize: 16,
    textAlign: 'center',
    fontStyle: 'italic',
  },
  buttonContainer: {
    width: '100%',
    gap: 16,
  },
  primaryButton: {
    backgroundColor: '#4CAF50',
    paddingVertical: 16,
    borderRadius: 12,
    alignItems: 'center',
    shadowColor: '#4CAF50',
    shadowOffset: {
      width: 0,
      height: 2,
    },
    shadowOpacity: 0.3,
    shadowRadius: 3.84,
    elevation: 5,
  },
  secondaryButton: {
    backgroundColor: '#3d3d3d',
    paddingVertical: 16,
    borderRadius: 12,
    alignItems: 'center',
    borderWidth: 1,
    borderColor: '#555555',
  },
  buttonText: {
    color: '#ffffff',
    fontSize: 16,
    fontWeight: '600',
    letterSpacing: 0.5,
  },
  secondaryButtonText: {
    color: '#dddddd',
    fontSize: 16,
    fontWeight: '600',
    letterSpacing: 0.5,
  },
});

export default function App() {
  const [mnemonic, setMnemonic] = React.useState<string>('');
  const [loading, setLoading] = React.useState(false);

  React.useEffect(() => {
    // Initialize things if needed, but don't auto-generate
    const init = async () => {
      await initFedimintCLI();
    };
    init();
  }, []);

  const handleGenerate = async () => {
    setLoading(true);
    try {
      const words = await director.generateMnemonic();
      setMnemonic(words.join(' '));
    } catch (e) {
      console.error(e);
    } finally {
      setLoading(false);
    }
  };

  const handleClear = () => {
    setMnemonic('');
  };

  return (
    <View style={styles.container}>
      <View style={styles.content}>
        <Text style={styles.header}>Fedimint Wallet</Text>

        <View style={styles.card}>
          <Text style={styles.mnemonicLabel}>Generated Mnemonic</Text>
          {mnemonic ? (
            <Text style={styles.mnemonicText}>{mnemonic}</Text>
          ) : (
            <Text style={styles.placeholderText}>
              Tap generate to create a new wallet seed
            </Text>
          )}
        </View>

        <View style={styles.buttonContainer}>
          <TouchableOpacity
            style={styles.primaryButton}
            onPress={handleGenerate}
            disabled={loading}
          >
            <Text style={styles.buttonText}>
              {loading ? 'Generating...' : 'Generate New Mnemonic'}
            </Text>
          </TouchableOpacity>

          <TouchableOpacity
            style={styles.secondaryButton}
            onPress={handleClear}
          >
            <Text style={styles.secondaryButtonText}>Clear Display</Text>
          </TouchableOpacity>
        </View>
      </View>
    </View>
  );
}
