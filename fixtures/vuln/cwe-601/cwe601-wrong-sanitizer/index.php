<?php

$value = htmlspecialchars($_GET['x']);
header('Location: ' . $value);
