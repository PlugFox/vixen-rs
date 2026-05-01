#[cfg(test)]
mod captcha_tests {
    use crate::captcha::CaptchaService;

    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::task::JoinSet;

    #[tokio::test]
    async fn test_single_captcha_generation() {
        let service = CaptchaService::new();
        let captcha = service.generate().await;

        // Проверяем базовые свойства
        assert_eq!(captcha.numbers.len(), 4);
        assert_eq!(captcha.text.len(), 4);
        assert!(!captcha.bytes.is_empty());
        assert_eq!(captcha.width, 480);
        assert_eq!(captcha.height, 180);

        // Проверяем, что text соответствует numbers
        let expected_text: String = captcha
            .numbers
            .iter()
            .map(|&n| (n + b'0') as char)
            .collect();
        assert_eq!(captcha.text, expected_text);

        // Проверяем, что все цифры в пределах 0-9
        for &num in &captcha.numbers {
            assert!(num < 10);
        }
    }

    #[tokio::test]
    async fn test_captcha_generation_with_transparency() {
        let service = CaptchaService::new();
        let captcha = service.generate_with_transparency(128).await;

        // Проверяем базовые свойства
        assert_eq!(captcha.numbers.len(), 4);
        assert_eq!(captcha.text.len(), 4);
        assert!(!captcha.bytes.is_empty());
        assert_eq!(captcha.width, 480);
        assert_eq!(captcha.height, 180);

        // Проверяем, что text соответствует numbers
        let expected_text: String = captcha
            .numbers
            .iter()
            .map(|&n| (n + b'0') as char)
            .collect();
        assert_eq!(captcha.text, expected_text);
    }

    #[tokio::test]
    async fn test_sequential_captcha_generation_no_deadlock() {
        let service = CaptchaService::new();
        let start_time = Instant::now();

        // Генерируем 10 CAPTCHA подряд
        for i in 0..10 {
            let captcha = service.generate().await;

            // Проверяем, что каждая CAPTCHA валидна
            assert_eq!(captcha.numbers.len(), 4);
            assert_eq!(captcha.text.len(), 4);
            assert!(!captcha.bytes.is_empty());

            println!("Generated CAPTCHA #{}: {}", i + 1, captcha.text);
        }

        let elapsed = start_time.elapsed();
        println!("Sequential generation took: {elapsed:?}");

        // Проверяем, что это не заняло слишком много времени (не должно быть дедлоков)
        assert!(
            elapsed < Duration::from_secs(30),
            "Sequential generation took too long: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_concurrent_captcha_generation_no_deadlock() {
        let service = Arc::new(CaptchaService::new());
        let start_time = Instant::now();
        let mut join_set = JoinSet::new();

        // Запускаем 10 одновременных задач генерации CAPTCHA
        for i in 0..10 {
            let service_clone = Arc::clone(&service);
            join_set.spawn(async move {
                let captcha = service_clone.generate().await;

                // Проверяем, что CAPTCHA валидна
                assert_eq!(captcha.numbers.len(), 4);
                assert_eq!(captcha.text.len(), 4);
                assert!(!captcha.bytes.is_empty());
                assert_eq!(captcha.width, 480);
                assert_eq!(captcha.height, 180);

                // Проверяем, что text соответствует numbers
                let expected_text: String = captcha
                    .numbers
                    .iter()
                    .map(|&n| (n + b'0') as char)
                    .collect();
                assert_eq!(captcha.text, expected_text);

                (i, captcha.text)
            });
        }

        // Ждем завершения всех задач
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let (task_id, text) = result.expect("Task should complete successfully");
            results.push((task_id, text));
        }

        let elapsed = start_time.elapsed();
        println!("Concurrent generation took: {elapsed:?}");

        // Проверяем, что все задачи завершились
        assert_eq!(results.len(), 10);

        // Проверяем, что это не заняло слишком много времени (не должно быть дедлоков)
        assert!(
            elapsed < Duration::from_secs(60),
            "Concurrent generation took too long: {elapsed:?}"
        );

        // Выводим результаты для отладки
        results.sort_by_key(|(id, _)| *id);
        for (id, text) in results {
            println!("Task #{id} generated CAPTCHA: {text}");
        }
    }

    #[tokio::test]
    async fn test_mixed_generation_methods() {
        let service = Arc::new(CaptchaService::new());
        let start_time = Instant::now();
        let mut join_set = JoinSet::new();

        // Запускаем смешанные задачи: обычные и с прозрачностью
        for i in 0..5 {
            let service_clone = Arc::clone(&service);
            join_set.spawn(async move {
                let captcha = service_clone.generate().await;
                (format!("normal_{i}"), captcha.text)
            });
        }

        for i in 0..5 {
            let service_clone = Arc::clone(&service);
            let alpha = 50 + i * 40; // Разные уровни прозрачности
            join_set.spawn(async move {
                let captcha = service_clone.generate_with_transparency(alpha as u8).await;
                (format!("transparent_{i}_{alpha}"), captcha.text)
            });
        }

        // Ждем завершения всех задач
        let mut results = Vec::new();
        while let Some(result) = join_set.join_next().await {
            let (method, text) = result.expect("Task should complete successfully");
            results.push((method, text));
        }

        let elapsed = start_time.elapsed();
        println!("Mixed generation took: {elapsed:?}");

        // Проверяем, что все задачи завершились
        assert_eq!(results.len(), 10);

        // Проверяем, что это не заняло слишком много времени
        assert!(
            elapsed < Duration::from_secs(60),
            "Mixed generation took too long: {elapsed:?}"
        );

        // Выводим результаты для отладки
        for (method, text) in results {
            println!("Method '{method}' generated CAPTCHA: {text}");
        }
    }

    #[tokio::test]
    async fn test_stress_generation() {
        let service = Arc::new(CaptchaService::new());
        let start_time = Instant::now();
        let mut join_set = JoinSet::new();

        // Стресс-тест: 50 одновременных генераций
        for i in 0..50 {
            let service_clone = Arc::clone(&service);
            join_set.spawn(async move {
                let captcha = service_clone.generate().await;

                // Базовые проверки
                assert_eq!(captcha.numbers.len(), 4);
                assert_eq!(captcha.text.len(), 4);
                assert!(!captcha.bytes.is_empty());

                i
            });
        }

        // Ждем завершения всех задач
        let mut completed = 0;
        while let Some(result) = join_set.join_next().await {
            result.expect("Task should complete successfully");
            completed += 1;
        }

        let elapsed = start_time.elapsed();
        println!("Stress test with {completed} generations took: {elapsed:?}");

        // Проверяем, что все задачи завершились
        assert_eq!(completed, 50);

        // Проверяем, что это не заняло слишком много времени (даже для 50 генераций)
        assert!(
            elapsed < Duration::from_secs(120),
            "Stress test took too long: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn test_captcha_uniqueness() {
        let service = CaptchaService::new();
        let mut generated_texts = std::collections::HashSet::new();

        // Генерируем 20 CAPTCHA и проверяем, что они разные
        for _ in 0..20 {
            let captcha = service.generate().await;
            generated_texts.insert(captcha.text);
        }

        // Поскольку это случайная генерация, вероятность получить 20 одинаковых CAPTCHA крайне мала
        // При 4 цифрах у нас 10^4 = 10000 возможных комбинаций
        assert!(
            generated_texts.len() > 15,
            "Generated CAPTCHA texts are not unique enough: {} unique out of 20",
            generated_texts.len()
        );

        println!(
            "Generated {count} unique CAPTCHA texts out of 20",
            count = generated_texts.len()
        );
    }
}
