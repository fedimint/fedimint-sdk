import * as React from 'react';

import { View, Text, TouchableOpacity } from 'react-native';
import WalletDirector from '@fedimint/react-native';
import RNFS from 'react-native-fs';
import styles from './styles';

let director: WalletDirector;

const initFedimintCLI = async () => {
  const dbPath = `${RNFS.DocumentDirectoryPath}/fedimint_db`;

  console.log('Using DB Path:', dbPath);

  director = new WalletDirector(dbPath);
};

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
